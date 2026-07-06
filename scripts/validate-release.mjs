/* global URL, console, process */

import { createHash } from "node:crypto";
import { access, lstat, readdir, readFile, readlink } from "node:fs/promises";
import { isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

import { PINNED_DEPENDENCIES } from "../packages/core/src/pinned-dependencies.mjs";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));

const pluginSpecs = [
  {
    name: "claude-code-plugin",
    packagePath: "packages/claude-code-plugin/package.json",
    manifestPath: "packages/claude-code-plugin/.claude-plugin/plugin.json",
    entryDir: "packages/claude-code-plugin",
    expectedHost: "claude-code",
  },
  {
    name: "codex-plugin",
    packagePath: "packages/codex-plugin/package.json",
    manifestPath: "packages/codex-plugin/.codex-plugin/plugin.json",
    entryDir: "packages/codex-plugin",
    expectedHost: "codex",
  },
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
      localModifications: "None; adapters live outside upstream/.",
    },
  },
];

export async function validateReleaseLayout(root = repoRoot, options = {}) {
  const platform = options.platform ?? process.platform;
  for (const spec of pluginSpecs) {
    const manifest = JSON.parse(
      await readFile(join(root, spec.manifestPath), "utf8"),
    );
    const packageJson = JSON.parse(
      await readFile(join(root, spec.packagePath), "utf8"),
    );

    assertString(manifest.version, `${spec.name} manifest.version`);
    assertString(manifest.entry, `${spec.name} manifest.entry`);
    assertObject(manifest.compatibility, `${spec.name} manifest.compatibility`);
    assertString(
      manifest.compatibility.corePackage,
      `${spec.name} compatibility.corePackage`,
    );
    assertString(
      manifest.compatibility.coreVersion,
      `${spec.name} compatibility.coreVersion`,
    );
    assertArray(
      manifest.compatibility.hosts,
      `${spec.name} compatibility.hosts`,
    );
    assertArray(manifest.compatibility.os, `${spec.name} compatibility.os`);

    if (manifest.compatibility.corePackage !== "@sdd-harness/core") {
      throw new Error(
        `${spec.name} compatibility.corePackage 必须是 @sdd-harness/core`,
      );
    }
    if (
      manifest.compatibility.coreVersion !==
      packageJson.dependencies?.["@sdd-harness/core"]
    ) {
      throw new Error(
        `${spec.name} compatibility.coreVersion 必须与 package.json 里的 @sdd-harness/core 依赖一致`,
      );
    }
    if (manifest.version !== packageJson.version) {
      throw new Error(
        `${spec.name} manifest.version 必须与 package.json version 一致`,
      );
    }
    if (!manifest.compatibility.hosts.includes(spec.expectedHost)) {
      throw new Error(
        `${spec.name} compatibility.hosts 必须包含 ${spec.expectedHost}`,
      );
    }
    const allowedOs = ["macos", "windows"];
    for (const os of ["macos", "windows"]) {
      if (!manifest.compatibility.os.includes(os)) {
        throw new Error(`${spec.name} compatibility.os 必须包含 ${os}`);
      }
    }
    for (const os of manifest.compatibility.os) {
      if (!allowedOs.includes(os)) {
        throw new Error(
          `${spec.name} compatibility.os 只允许声明 macos 和 windows`,
        );
      }
    }

    await ensureReadable(join(root, spec.entryDir, manifest.entry));
  }

  for (const spec of vendorSpecs) {
    await validateVendorSnapshot(root, spec, platform);
  }
}

function pickVersionMetadata(dependency) {
  const { name, version, commit, repository, license } = dependency;
  return { name, version, commit, repository, license };
}

async function validateVendorSnapshot(root, spec, platform) {
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
      if (
        platform === "win32" &&
        expected.type === "symlink" &&
        actual.type === "file"
      ) {
        const placeholder = await readFile(join(upstreamRoot, path), "utf8");
        if (placeholder !== expected.target) {
          throw new Error(
            `${spec.directory} Windows 符号链接占位文件内容不一致：upstream/${path}`,
          );
        }
      } else {
        throw new Error(
          `${spec.directory} 快照条目类型不一致：upstream/${path}`,
        );
      }
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

function assertString(value, field) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${field} 必须是非空字符串`);
  }
}

function assertArray(value, field) {
  if (!Array.isArray(value) || value.length === 0) {
    throw new Error(`${field} 必须是非空数组`);
  }
}

function assertObject(value, field) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${field} 必须是对象`);
  }
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
