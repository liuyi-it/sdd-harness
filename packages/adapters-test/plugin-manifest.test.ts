import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

interface PluginManifest {
  entry: string;
  compatibility: {
    corePackage: string;
    coreVersion: string;
    hosts: string[];
    os: string[];
  };
}

describe("plugin manifests", () => {
  it.each([
    {
      pluginDir: "packages/claude-code-plugin",
      manifestPath: ".claude-plugin/plugin.json",
      expectedHost: "claude-code",
    },
    {
      pluginDir: "packages/codex-plugin",
      manifestPath: ".codex-plugin/plugin.json",
      expectedHost: "codex",
    },
  ])(
    "declares core compatibility, entry, and target platforms for %s",
    async ({ pluginDir, manifestPath, expectedHost }) => {
      const root = process.cwd();
      const manifest = JSON.parse(
        await readFile(join(root, pluginDir, manifestPath), "utf8"),
      ) as PluginManifest;
      const packageJson = JSON.parse(
        await readFile(join(root, pluginDir, "package.json"), "utf8"),
      ) as {
        version: string;
        dependencies: Record<string, string>;
      };

      expect(manifest.entry).toBe("./src/index.ts");
      await expect(
        readFile(join(root, pluginDir, manifest.entry), "utf8"),
      ).resolves.toContain("export");
      expect(manifest.compatibility).toMatchObject({
        corePackage: "@sdd-harness/core",
        coreVersion: packageJson.dependencies["@sdd-harness/core"],
        hosts: [expectedHost],
        os: ["macos", "windows"],
      });
    },
  );
});
