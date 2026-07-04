/* global console, URL */

import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { validateReleaseLayout } from "./validate-release.mjs";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));

export async function prepareClaudeInstall(options = {}) {
  const root = resolveRepoRoot(options.repoRoot);
  await validateInstallSource(root);
  await ensureReadableJson(
    join(root, ".claude-plugin", "marketplace.json"),
    "缺少 Claude Code marketplace 文件",
  );

  return {
    repoRoot: root,
    commands: [
      `/plugin marketplace add ${root}`,
      "/plugin install sdd-harness@sdd-harness",
    ],
    verifyCommands: ["/sdd.init", "/sdd.status"],
  };
}

export async function installCodexPlugin(options = {}) {
  const root = resolveRepoRoot(options.repoRoot);
  const targetHome = resolve(options.homeDir ?? homedir());
  const pluginPath = join(targetHome, ".codex", "plugins", "sdd-harness");
  const marketplacePath = join(
    targetHome,
    ".agents",
    "plugins",
    "marketplace.json",
  );

  await validateInstallSource(root);
  await rm(pluginPath, { recursive: true, force: true });
  await mkdir(dirname(pluginPath), { recursive: true });
  await cp(join(root, "packages", "codex-plugin"), pluginPath, {
    recursive: true,
  });

  const marketplace = await readMarketplace(marketplacePath);
  const plugins = Array.isArray(marketplace.plugins) ? marketplace.plugins : [];
  const nextPlugin = {
    name: "sdd-harness",
    source: {
      source: "local",
      path: "./.codex/plugins/sdd-harness",
    },
    policy: {
      installation: "AVAILABLE",
      authentication: "ON_INSTALL",
    },
    category: "Productivity",
  };
  const nextPlugins = [
    ...plugins.filter((plugin) => plugin?.name !== "sdd-harness"),
    nextPlugin,
  ];

  await mkdir(dirname(marketplacePath), { recursive: true });
  await writeFile(
    marketplacePath,
    `${JSON.stringify(
      {
        name:
          typeof marketplace.name === "string"
            ? marketplace.name
            : "local-personal",
        plugins: nextPlugins,
      },
      null,
      2,
    )}\n`,
    "utf8",
  );

  return {
    pluginPath,
    marketplacePath,
    verifyCommands: ["sdd init", "sdd status"],
  };
}

export async function validateInstallSource(root = repoRoot) {
  await validateReleaseLayout(root);
}

export function resolveRepoRoot(root = repoRoot) {
  return resolve(root);
}

export function printClaudeInstallSummary(plan, output = console.log) {
  output("Claude Code 本地安装准备完成。");
  output("");
  output("请在 Claude Code 会话中依次执行：");
  for (const command of plan.commands) output(command);
  output("");
  output("最后一步宿主内安装/重载完成后，可用以下命令验证：");
  for (const command of plan.verifyCommands) output(command);
}

export function printCodexInstallSummary(result, output = console.log) {
  output("Codex 本地插件安装完成。");
  output(`插件目录：${result.pluginPath}`);
  output(`marketplace：${result.marketplacePath}`);
  output("");
  output("重启 Codex 后，可用以下命令验证：");
  for (const command of result.verifyCommands) output(command);
}

async function ensureReadableJson(path, message) {
  try {
    JSON.parse(await readFile(path, "utf8"));
  } catch {
    throw new Error(`${message}：${path}`);
  }
}

async function readMarketplace(path) {
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch {
    return { name: "local-personal", plugins: [] };
  }
}
