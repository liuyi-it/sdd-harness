import { cp, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { validateReleaseLayout } from "../../scripts/validate-release.mjs";

const roots: string[] = [];

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("release validation", () => {
  it("passes for the current repository layout", async () => {
    await expect(validateReleaseLayout(process.cwd())).resolves.toBeUndefined();
  });

  it("fails when a plugin entry file is missing", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-release-"));
    roots.push(root);
    await mkdir(join(root, "packages/claude-code-plugin/.claude-plugin"), {
      recursive: true,
    });
    await mkdir(join(root, "packages/codex-plugin/.codex-plugin"), {
      recursive: true,
    });
    await writeFile(
      join(root, "packages/claude-code-plugin/package.json"),
      JSON.stringify({
        version: "0.1.0",
        dependencies: { "@sdd-harness/core": "0.1.0" },
      }),
      "utf8",
    );
    await writeFile(
      join(root, "packages/codex-plugin/package.json"),
      JSON.stringify({
        version: "0.1.0",
        dependencies: { "@sdd-harness/core": "0.1.0" },
      }),
      "utf8",
    );
    await writeFile(
      join(root, "packages/claude-code-plugin/.claude-plugin/plugin.json"),
      JSON.stringify({
        version: "0.1.0",
        entry: "./src/index.ts",
        compatibility: {
          corePackage: "@sdd-harness/core",
          coreVersion: "0.1.0",
          hosts: ["claude-code"],
          os: ["macos", "windows"],
        },
      }),
      "utf8",
    );
    await writeFile(
      join(root, "packages/codex-plugin/.codex-plugin/plugin.json"),
      JSON.stringify({
        version: "0.1.0",
        entry: "./src/index.ts",
        compatibility: {
          corePackage: "@sdd-harness/core",
          coreVersion: "0.1.0",
          hosts: ["codex"],
          os: ["macos", "windows"],
        },
      }),
      "utf8",
    );
    await mkdir(join(root, "packages/codex-plugin/src"), { recursive: true });
    await writeFile(
      join(root, "packages/codex-plugin/src/index.ts"),
      "export {};",
      "utf8",
    );

    await expect(validateReleaseLayout(root)).rejects.toThrow(
      /缺少发布必需文件/,
    );
  });

  it("fails when manifest.version does not match package.json version", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-release-"));
    roots.push(root);
    await mkdir(join(root, "packages/claude-code-plugin/.claude-plugin"), {
      recursive: true,
    });
    await mkdir(join(root, "packages/codex-plugin/.codex-plugin"), {
      recursive: true,
    });
    await mkdir(join(root, "packages/claude-code-plugin/src"), {
      recursive: true,
    });
    await mkdir(join(root, "packages/codex-plugin/src"), { recursive: true });
    await writeFile(
      join(root, "packages/claude-code-plugin/package.json"),
      JSON.stringify({
        version: "0.1.0",
        dependencies: { "@sdd-harness/core": "0.1.0" },
      }),
      "utf8",
    );
    await writeFile(
      join(root, "packages/codex-plugin/package.json"),
      JSON.stringify({
        version: "0.1.0",
        dependencies: { "@sdd-harness/core": "0.1.0" },
      }),
      "utf8",
    );
    await writeFile(
      join(root, "packages/claude-code-plugin/.claude-plugin/plugin.json"),
      JSON.stringify({
        version: "9.9.9",
        entry: "./src/index.ts",
        compatibility: {
          corePackage: "@sdd-harness/core",
          coreVersion: "0.1.0",
          hosts: ["claude-code"],
          os: ["macos", "windows"],
        },
      }),
      "utf8",
    );
    await writeFile(
      join(root, "packages/codex-plugin/.codex-plugin/plugin.json"),
      JSON.stringify({
        version: "0.1.0",
        entry: "./src/index.ts",
        compatibility: {
          corePackage: "@sdd-harness/core",
          coreVersion: "0.1.0",
          hosts: ["codex"],
          os: ["macos", "windows"],
        },
      }),
      "utf8",
    );
    await writeFile(
      join(root, "packages/claude-code-plugin/src/index.ts"),
      "export {};",
      "utf8",
    );
    await writeFile(
      join(root, "packages/codex-plugin/src/index.ts"),
      "export {};",
      "utf8",
    );

    await expect(validateReleaseLayout(root)).rejects.toThrow(
      /manifest\.version 必须与 package\.json version 一致/,
    );
  });

  it("fails when a manifest declares unsupported operating systems", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-release-"));
    roots.push(root);
    await mkdir(join(root, "packages/claude-code-plugin/.claude-plugin"), {
      recursive: true,
    });
    await mkdir(join(root, "packages/codex-plugin/.codex-plugin"), {
      recursive: true,
    });
    await mkdir(join(root, "packages/claude-code-plugin/src"), {
      recursive: true,
    });
    await mkdir(join(root, "packages/codex-plugin/src"), { recursive: true });
    for (const plugin of ["claude-code-plugin", "codex-plugin"] as const) {
      await writeFile(
        join(root, "packages", plugin, "package.json"),
        JSON.stringify({
          version: "0.1.0",
          dependencies: { "@sdd-harness/core": "0.1.0" },
        }),
        "utf8",
      );
      await writeFile(
        join(root, "packages", plugin, "src/index.ts"),
        "export {};",
        "utf8",
      );
    }
    await writeFile(
      join(root, "packages/claude-code-plugin/.claude-plugin/plugin.json"),
      JSON.stringify({
        version: "0.1.0",
        entry: "./src/index.ts",
        compatibility: {
          corePackage: "@sdd-harness/core",
          coreVersion: "0.1.0",
          hosts: ["claude-code"],
          os: ["macos", "windows", "linux"],
        },
      }),
      "utf8",
    );
    await writeFile(
      join(root, "packages/codex-plugin/.codex-plugin/plugin.json"),
      JSON.stringify({
        version: "0.1.0",
        entry: "./src/index.ts",
        compatibility: {
          corePackage: "@sdd-harness/core",
          coreVersion: "0.1.0",
          hosts: ["codex"],
          os: ["macos", "windows"],
        },
      }),
      "utf8",
    );

    await expect(validateReleaseLayout(root)).rejects.toThrow(
      /compatibility\.os 只允许声明 macos 和 windows/,
    );
  });

  it("fails when an upstream snapshot file digest does not match", async () => {
    const root = await makeReleaseCopy();
    await mkdir(join(root, "vendor/openspec/upstream"), { recursive: true });
    await writeFile(
      join(root, "vendor/openspec/upstream/LICENSE"),
      "tampered\n",
      "utf8",
    );

    await expect(validateReleaseLayout(root)).rejects.toThrow(/摘要不一致/);
  });

  it("fails when an upstream snapshot contains an unlisted file", async () => {
    const root = await makeReleaseCopy();
    await mkdir(join(root, "vendor/superpowers/upstream"), { recursive: true });
    await writeFile(
      join(root, "vendor/superpowers/upstream/EXTRA"),
      "unexpected\n",
      "utf8",
    );

    await expect(validateReleaseLayout(root)).rejects.toThrow(/清单外文件/);
  });
});

async function makeReleaseCopy() {
  const root = await mkdtemp(join(tmpdir(), "sdd-release-vendor-"));
  roots.push(root);
  for (const path of [
    "packages/claude-code-plugin/package.json",
    "packages/claude-code-plugin/.claude-plugin/plugin.json",
    "packages/claude-code-plugin/src/index.ts",
    "packages/codex-plugin/package.json",
    "packages/codex-plugin/.codex-plugin/plugin.json",
    "packages/codex-plugin/src/index.ts",
  ]) {
    await mkdir(join(root, path, ".."), { recursive: true });
    await writeFile(
      join(root, path),
      await readFile(join(process.cwd(), path)),
    );
  }
  await cp(join(process.cwd(), "vendor"), join(root, "vendor"), {
    recursive: true,
  });
  return root;
}
