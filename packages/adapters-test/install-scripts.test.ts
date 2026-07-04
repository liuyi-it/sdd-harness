import { access, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import {
  installCodexPlugin,
  prepareClaudeInstall,
} from "../../scripts/install-shared.mjs";

const roots: string[] = [];

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true, force: true })),
  );
});

describe("install scripts", () => {
  it("为 Claude Code 生成当前仓库的一键安装指引", async () => {
    const root = await createPluginRepo();

    const plan = await prepareClaudeInstall({ repoRoot: root });

    expect(plan.repoRoot).toBe(root);
    expect(plan.commands).toEqual([
      `/plugin marketplace add ${root}`,
      "/plugin install sdd-harness@sdd-harness",
    ]);
    expect(plan.verifyCommands).toContain("/sdd.init");
    expect(plan.verifyCommands).toContain("/sdd.status");
  });

  it("Claude Code 安装指引在缺少 marketplace 文件时失败", async () => {
    const root = await createPluginRepo();
    await rmPath(join(root, ".claude-plugin", "marketplace.json"));

    await expect(prepareClaudeInstall({ repoRoot: root })).rejects.toThrow(
      /缺少 Claude Code marketplace 文件/,
    );
  });

  it("为 Codex 安装插件并写入本地 marketplace", async () => {
    const root = await createPluginRepo();
    const homeDir = await mkdtemp(join(tmpdir(), "sdd-home-"));
    roots.push(homeDir);

    const result = await installCodexPlugin({ repoRoot: root, homeDir });

    await expect(
      access(join(homeDir, ".codex", "plugins", "sdd-harness", "package.json")),
    ).resolves.toBeUndefined();

    const marketplace = JSON.parse(
      await readFile(
        join(homeDir, ".agents", "plugins", "marketplace.json"),
        "utf8",
      ),
    ) as {
      plugins: Array<{ name: string; source: { path: string } }>;
    };

    expect(result.pluginPath).toBe(
      join(homeDir, ".codex", "plugins", "sdd-harness"),
    );
    expect(result.marketplacePath).toBe(
      join(homeDir, ".agents", "plugins", "marketplace.json"),
    );
    expect(marketplace.plugins).toHaveLength(1);
    expect(marketplace.plugins[0]).toMatchObject({
      name: "sdd-harness",
      source: {
        path: "./.codex/plugins/sdd-harness",
      },
    });
  });

  it("Codex 安装会幂等更新已有 sdd-harness 条目", async () => {
    const root = await createPluginRepo();
    const homeDir = await mkdtemp(join(tmpdir(), "sdd-home-"));
    roots.push(homeDir);
    await mkdir(join(homeDir, ".agents", "plugins"), { recursive: true });
    await writeFile(
      join(homeDir, ".agents", "plugins", "marketplace.json"),
      JSON.stringify({
        name: "local-personal",
        plugins: [
          {
            name: "other-plugin",
            source: { source: "local", path: "./.codex/plugins/other-plugin" },
            policy: {
              installation: "AVAILABLE",
              authentication: "ON_INSTALL",
            },
            category: "Productivity",
          },
          {
            name: "sdd-harness",
            source: { source: "local", path: "./old/path" },
            policy: {
              installation: "AVAILABLE",
              authentication: "ON_INSTALL",
            },
            category: "Old",
          },
        ],
      }),
      "utf8",
    );

    await installCodexPlugin({ repoRoot: root, homeDir });

    const marketplace = JSON.parse(
      await readFile(
        join(homeDir, ".agents", "plugins", "marketplace.json"),
        "utf8",
      ),
    ) as {
      plugins: Array<{
        name: string;
        category: string;
        source: { path: string };
      }>;
    };

    expect(marketplace.plugins).toHaveLength(2);
    expect(
      marketplace.plugins.filter((plugin) => plugin.name === "sdd-harness"),
    ).toHaveLength(1);
    expect(
      marketplace.plugins.find((plugin) => plugin.name === "sdd-harness"),
    ).toMatchObject({
      category: "Productivity",
      source: {
        path: "./.codex/plugins/sdd-harness",
      },
    });
  });
});

async function createPluginRepo() {
  const root = await mkdtemp(join(tmpdir(), "sdd-install-"));
  roots.push(root);
  await mkdir(join(root, ".claude-plugin"), { recursive: true });
  await mkdir(join(root, "packages", "claude-code-plugin", ".claude-plugin"), {
    recursive: true,
  });
  await mkdir(join(root, "packages", "codex-plugin", ".codex-plugin"), {
    recursive: true,
  });
  await mkdir(join(root, "packages", "claude-code-plugin", "dist"), {
    recursive: true,
  });
  await mkdir(join(root, "packages", "codex-plugin", "dist"), {
    recursive: true,
  });
  await writeFile(
    join(root, ".claude-plugin", "marketplace.json"),
    JSON.stringify({
      plugins: [
        {
          name: "sdd-harness",
          source: "./packages/claude-code-plugin",
        },
      ],
    }),
    "utf8",
  );
  await writeFile(
    join(root, "packages", "claude-code-plugin", "package.json"),
    JSON.stringify({
      name: "@sdd-harness/claude-code-plugin",
      version: "0.1.0",
      dependencies: { "@sdd-harness/core": "0.1.0" },
    }),
    "utf8",
  );
  await writeFile(
    join(root, "packages", "codex-plugin", "package.json"),
    JSON.stringify({
      name: "@sdd-harness/codex-plugin",
      version: "0.1.0",
      dependencies: { "@sdd-harness/core": "0.1.0" },
    }),
    "utf8",
  );
  await writeFile(
    join(
      root,
      "packages",
      "claude-code-plugin",
      ".claude-plugin",
      "plugin.json",
    ),
    JSON.stringify({
      version: "0.1.0",
      entry: "./dist/index.js",
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
    join(root, "packages", "codex-plugin", ".codex-plugin", "plugin.json"),
    JSON.stringify({
      version: "0.1.0",
      entry: "./dist/index.js",
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
    join(root, "packages", "claude-code-plugin", "dist", "index.js"),
    "export {};",
    "utf8",
  );
  await writeFile(
    join(root, "packages", "codex-plugin", "dist", "index.js"),
    "export {};",
    "utf8",
  );

  return root;
}

async function rmPath(path: string) {
  const { rm } = await import("node:fs/promises");
  await rm(path, { force: true });
}
