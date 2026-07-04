/* global URL, console, process */

import { access, readFile } from "node:fs/promises";
import { join } from "node:path";
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
