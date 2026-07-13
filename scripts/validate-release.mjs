/* global URL, console, process */

import { createHash } from "node:crypto";
import { access, lstat, readdir, readFile, readlink } from "node:fs/promises";
import { isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

import { PINNED_DEPENDENCIES } from "../packages/core/src/pinned-dependencies.mjs";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));

/** 三期必须发布的包（无需 npm，本地 link 安装） */
const REQUIRED_PACKAGES = [
  "core",
  "cli",
  "agent-protocol",
  "agent-policies",
  "codebase-memory",
  "claude-code-adapter",
  "codex-adapter",
  "opencode-adapter",
  "generic-agent-adapter",
];

const vendorSpecs = [
  {
    directory: "openspec",
    metadata: {
      ...pickVersionMetadata(PINNED_DEPENDENCIES.openSpec),
      localModifications: "None; adapters live outside upstream/.",
    },
  },
  {
    directory: "superpowers",
    metadata: {
      ...pickVersionMetadata(PINNED_DEPENDENCIES.superpowers),
      localModifications:
        "AGENTS.md materialized as a regular copy of CLAUDE.md for Windows checkout compatibility; adapters live outside upstream/.",
    },
  },
];

export async function validateReleaseLayout(root = repoRoot) {
  // 校验根 package.json
  const rootPkg = JSON.parse(
    await readFile(join(root, "package.json"), "utf8"),
  );
  if (rootPkg.engines?.node !== ">=22") {
    throw new Error("根 package.json engines.node 必须为 >=22");
  }

  // 校验所有必需包存在且 engines >=22
  for (const pkgName of REQUIRED_PACKAGES) {
    const pkgPath = join(root, "packages", pkgName, "package.json");
    await ensureReadable(pkgPath);
    const pkg = JSON.parse(await readFile(pkgPath, "utf8"));
    if (pkg.version !== "0.1.0") {
      throw new Error(`packages/${pkgName} version 必须为 0.1.0`);
    }
    if (pkg.engines?.node !== ">=22") {
      throw new Error(`packages/${pkgName} engines.node 必须为 >=22`);
    }
  }

  // 校验 vendor 快照
  for (const spec of vendorSpecs) {
    await validateVendorSnapshot(root, spec);
  }

  await validatePolicyUpstream(root);
}

async function validatePolicyUpstream(root) {
  const dependency = PINNED_DEPENDENCIES.mattpocockSkills;
  const vendorRoot = join(root, "vendor", "mattpocock-skills");
  const upstream = await readFile(join(vendorRoot, "UPSTREAM.md"), "utf8");
  for (const expected of [
    dependency.repository,
    dependency.commit,
    `License: ${dependency.license}`,
    "Imported files:",
    "Adapted policies:",
    "Local modifications:",
    "Last reviewed:",
  ]) {
    if (!upstream.includes(expected))
      throw new Error(`mattpocock-skills UPSTREAM.md 缺少：${expected}`);
  }
  await ensureReadable(join(vendorRoot, "LICENSE"));
  await ensureReadable(join(vendorRoot, "upstream", "LICENSE"));
}

function pickVersionMetadata(dependency) {
  const { name, version, commit, repository, license } = dependency;
  return { name, version, commit, repository, license };
}

async function validateVendorSnapshot(root, spec) {
  const vendorRoot = join(root, "vendor", spec.directory);
  const upstreamRoot = join(vendorRoot, "upstream");
  const metadata = JSON.parse(
    await readFile(join(vendorRoot, "VERSION.json"), "utf8"),
  );

  for (const [field, expected] of Object.entries(spec.metadata)) {
    if (metadata[field] !== expected) {
      throw new Error(
        `${spec.directory} VERSION.json 的 ${field} 必须是 ${expected}`,
      );
    }
  }

  await ensureReadable(join(vendorRoot, "LICENSE"));
  await ensureReadable(join(upstreamRoot, "LICENSE"));
  const manifest = parseManifest(
    await readFile(join(vendorRoot, "MANIFEST.sha256"), "utf8"),
    spec.directory,
    upstreamRoot,
  );
  const actualEntries = await listEntries(upstreamRoot);
  const actualByPath = new Map(
    actualEntries.map((entry) => [entry.path, entry]),
  );

  for (const path of manifest.keys()) {
    if (!actualByPath.has(path)) {
      throw new Error(`${spec.directory} 快照缺少清单文件：upstream/${path}`);
    }
  }
  for (const entry of actualEntries) {
    if (!manifest.has(entry.path)) {
      throw new Error(
        `${spec.directory} 快照存在清单外文件：upstream/${entry.path}`,
      );
    }
  }
  for (const [path, expected] of manifest) {
    const actual = actualByPath.get(path);
    if (actual.type !== expected.type) {
      throw new Error(`${spec.directory} 快照条目类型不一致：upstream/${path}`);
    }
    if (actual.type === "symlink" && actual.target !== expected.target) {
      throw new Error(
        `${spec.directory} 快照符号链接目标不一致：upstream/${path}`,
      );
    }
    const content =
      expected.type === "symlink"
        ? expected.target
        : await readFile(join(upstreamRoot, path));
    const digest = createHash("sha256").update(content).digest("hex");
    if (digest !== expected.digest) {
      throw new Error(`${spec.directory} 快照文件摘要不一致：upstream/${path}`);
    }
  }
}

function parseManifest(content, directory, upstreamRoot) {
  const entries = new Map();
  for (const line of content.trimEnd().split("\n")) {
    const fileMatch = /^([a-f0-9]{64}) {2}file upstream\/(.+)$/.exec(line);
    const symlinkMatch =
      /^([a-f0-9]{64}) {2}symlink upstream\/(.+) -> (".*")$/.exec(line);
    let entry;
    try {
      entry = fileMatch
        ? { digest: fileMatch[1], path: fileMatch[2], type: "file" }
        : symlinkMatch
          ? {
              digest: symlinkMatch[1],
              path: symlinkMatch[2],
              type: "symlink",
              target: JSON.parse(symlinkMatch[3]),
            }
          : undefined;
    } catch {
      entry = undefined;
    }
    if (!entry || !isSafeManifestPath(entry.path, upstreamRoot)) {
      throw new Error(`${directory} MANIFEST.sha256 包含无效条目：${line}`);
    }
    if (entry.type === "symlink" && typeof entry.target !== "string") {
      throw new Error(`${directory} MANIFEST.sha256 包含无效条目：${line}`);
    }
    if (entries.has(entry.path)) {
      throw new Error(
        `${directory} MANIFEST.sha256 包含重复条目：${entry.path}`,
      );
    }
    entries.set(entry.path, entry);
  }
  return entries;
}

function isSafeManifestPath(path, root) {
  if (
    path.includes("\\") ||
    path.includes("\0") ||
    /^[A-Za-z]:/.test(path) ||
    path.startsWith("/")
  ) {
    return false;
  }
  const components = path.split("/");
  if (
    components.some(
      (component) =>
        component === "" || component === "." || component === "..",
    )
  ) {
    return false;
  }
  const resolved = resolve(root, ...components);
  const relativePath = relative(root, resolved);
  return (
    !isAbsolute(relativePath) &&
    relativePath !== ".." &&
    !relativePath.startsWith(`..${sep}`) &&
    relativePath.split(sep).join("/") === path
  );
}

async function listEntries(root) {
  const entries = [];

  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const absolutePath = resolve(directory, entry.name);
      const stats = await lstat(absolutePath);
      if (stats.isDirectory()) {
        await visit(absolutePath);
      } else if (stats.isSymbolicLink()) {
        entries.push({
          path: relative(root, absolutePath).split(sep).join("/"),
          type: "symlink",
          target: await readlink(absolutePath),
        });
      } else if (stats.isFile()) {
        entries.push({
          path: relative(root, absolutePath).split(sep).join("/"),
          type: "file",
        });
      } else {
        throw new Error(
          `不支持的上游快照条目类型：${relative(root, absolutePath)}`,
        );
      }
    }
  }

  await visit(root);
  return entries.sort((left, right) =>
    left.path < right.path ? -1 : left.path > right.path ? 1 : 0,
  );
}

async function ensureReadable(path) {
  try {
    await access(path);
  } catch {
    throw new Error(`缺少发布必需文件：${path}`);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  validateReleaseLayout()
    .then(() => {
      console.log("release validation passed");
    })
    .catch((error) => {
      console.error(error.message);
      process.exitCode = 1;
    });
}
