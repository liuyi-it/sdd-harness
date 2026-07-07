#!/usr/bin/env node
// sdd / sdd-harness CLI 入口
import { parseArgs } from "node:util";
import { ExitCode } from "./exit-codes.js";

const PKG_VERSION = "0.1.0";

const HELP_TEXT = `sdd — SDD Agent Harness CLI

用法: sdd <command> [options]

命令:
  init              初始化 .sdd/
  status            显示当前 SDD 状态
  new <需求>         创建新变更
  design            生成设计制品
  plan              生成实施计划
  build             构建 (build next / build complete)
  verify            验证
  review            审查
  archive           归档
  auto <需求>        自动推进完整流程

通用参数:
  --json            JSON 输出
  --cwd <path>      项目根目录 (默认当前目录)
  --change <id>     指定变更 ID
  --timeout <s>     超时秒数
  --non-interactive 禁止交互
  --force           强制
  --verbose         详细输出
  --help            帮助
  --version         版本
`;

const COMMANDS = [
  "init",
  "status",
  "new",
  "design",
  "plan",
  "build",
  "verify",
  "review",
  "archive",
  "auto",
];

async function main(): Promise<void> {
  const { values, positionals } = parseArgs({
    args: process.argv.slice(2),
    options: {
      json: { type: "boolean", default: false },
      cwd: { type: "string" },
      change: { type: "string" },
      timeout: { type: "string" },
      "non-interactive": { type: "boolean", default: false },
      force: { type: "boolean", default: false },
      verbose: { type: "boolean", default: false },
      help: { type: "boolean", default: false },
      version: { type: "boolean", default: false },
    },
    allowPositionals: true,
  });

  const binName = process.env.SDD_BIN_NAME ?? "sdd";

  if (values.version) {
    console.log(`${binName} v${PKG_VERSION}`);
    process.exit(ExitCode.SUCCESS);
  }

  if (values.help || positionals.length === 0) {
    console.log(HELP_TEXT);
    process.exit(ExitCode.SUCCESS);
  }

  const [command] = positionals;
  if (!command || !COMMANDS.includes(command!)) {
    console.error(`未知命令: ${command ?? "(空)"}`);
    console.error(`可用命令: ${COMMANDS.join(", ")}`);
    process.exit(ExitCode.INVALID_ARGS);
  }

  // 后续阶段实现实际命令分发
  console.log(`Command: ${command} (not yet implemented)`);
  process.exit(ExitCode.SUCCESS);
}

main().catch((err) => {
  console.error("CLI 内部错误:", (err as Error).message);
  process.exit(ExitCode.GENERAL_ERROR);
});
