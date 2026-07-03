import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
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
      JSON.stringify({ dependencies: { "@sdd-harness/core": "0.1.0" } }),
      "utf8",
    );
    await writeFile(
      join(root, "packages/codex-plugin/package.json"),
      JSON.stringify({ dependencies: { "@sdd-harness/core": "0.1.0" } }),
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
});
