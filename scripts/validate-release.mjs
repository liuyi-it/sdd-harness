/* global URL, console, process */

import { createHash } from "node:crypto";
import { access, readdir, readFile } from "node:fs/promises";
import { join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

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
      name: "OpenSpec",
      version: "v1.4.1",
      commit: "1b06fddd59d8e592d5b5794a1970b22867e85b1f",
      repository: "https://github.com/Fission-AI/OpenSpec",
      license: "MIT",
      localModifications: "None; adapters live outside upstream/.",
    },
  },
  {
    directory: "superpowers",
    metadata: {
      name: "Superpowers",
      version: "v6.1.1",
      commit: "d884ae04edebef577e82ff7c4e143debd0bbec99",
      repository: "https://github.com/obra/superpowers",
      license: "MIT",
      localModifications: "None; adapters live outside upstream/.",
    },
  },
];

export async function validateReleaseLayout(root = repoRoot) {
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
    await validateVendorSnapshot(root, spec);
  }
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

  await ensureReadable(join(upstreamRoot, "LICENSE"));
  const manifest = parseManifest(
    await readFile(join(vendorRoot, "MANIFEST.sha256"), "utf8"),
    spec.directory,
  );
  const actualPaths = await listFiles(upstreamRoot);
  const actualSet = new Set(actualPaths);

  for (const path of manifest.keys()) {
    if (!actualSet.has(path)) {
      throw new Error(`${spec.directory} 快照缺少清单文件：upstream/${path}`);
    }
  }
  for (const path of actualPaths) {
    if (!manifest.has(path)) {
      throw new Error(`${spec.directory} 快照存在清单外文件：upstream/${path}`);
    }
  }
  for (const [path, expectedDigest] of manifest) {
    const digest = createHash("sha256")
      .update(await readFile(join(upstreamRoot, path)))
      .digest("hex");
    if (digest !== expectedDigest) {
      throw new Error(`${spec.directory} 快照文件摘要不一致：upstream/${path}`);
    }
  }
}

function parseManifest(content, directory) {
  const entries = new Map();
  for (const line of content.trimEnd().split("\n")) {
    const match = /^([a-f0-9]{64}) {2}upstream\/(.+)$/.exec(line);
    if (
      !match ||
      match[2].startsWith("/") ||
      match[2].split("/").includes("..")
    ) {
      throw new Error(`${directory} MANIFEST.sha256 包含无效条目：${line}`);
    }
    if (entries.has(match[2])) {
      throw new Error(`${directory} MANIFEST.sha256 包含重复条目：${match[2]}`);
    }
    entries.set(match[2], match[1]);
  }
  return entries;
}

async function listFiles(root) {
  const files = [];

  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const absolutePath = resolve(directory, entry.name);
      if (entry.isDirectory()) {
        await visit(absolutePath);
      } else {
        files.push(relative(root, absolutePath).split(sep).join("/"));
      }
    }
  }

  await visit(root);
  return files.sort();
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
