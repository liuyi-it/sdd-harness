#!/usr/bin/env node
// sdd / sdd-harness CLI 入口 — 参数解析、命令路由、输出格式化
import { parseArgs } from "node:util";
import { CodebaseAdapter, Core, type CommandResult } from "@sdd-harness/core";
import {
  CodebaseMemoryManager,
  CodebaseMemoryTransport,
} from "@sdd-harness/codebase-memory";
import { ExitCode } from "./exit-codes.js";
import { outputJson, outputText } from "./json-output.js";
import { runInit } from "./commands/init.js";
import { runStatus } from "./commands/status.js";
import { runNew } from "./commands/new.js";
import { runDesign } from "./commands/design.js";
import { runPlan } from "./commands/plan.js";
import { runBuild } from "./commands/build.js";
import { runVerify } from "./commands/verify.js";
import { runReview } from "./commands/review.js";
import { runArchive } from "./commands/archive.js";
import { runAuto } from "./commands/auto.js";
import { runCodebase } from "./commands/codebase.js";

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
  auto <需求>        自动推进 SDD Loop
  auto --resume     恢复当前 auto run
  auto --restart    重启 auto run
  auto --stop       停止当前 auto run
  auto --events     查看 auto run 事件
  auto --loop-status 查看 auto loop 状态
  status --loop     显示 loop 状态摘要
  codebase           代码库上下文管理 (status/doctor/index/query/rebuild)

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
  "codebase",
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
      task: { type: "string" },
      result: { type: "string" },
      intent: { type: "string" },
      agent: { type: "string" },
      structurePolicy: { type: "string" },
      host: { type: "string" },
      answers: { type: "string" },
      resume: { type: "boolean", default: false },
      restart: { type: "boolean", default: false },
      stop: { type: "boolean", default: false },
      events: { type: "boolean", default: false },
      tail: { type: "string" },
      loop: { type: "boolean", default: false },
      run: { type: "string" },
      "loop-status": { type: "boolean", default: false },
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

  // 构造 MCP transport → CodebaseAdapter → Core 的完整依赖链
  const codebaseManager = new CodebaseMemoryManager();
  const codebaseTransport = new CodebaseMemoryTransport(codebaseManager);
  const core = new Core({
    codebase: new CodebaseAdapter(codebaseTransport),
  });
  const cwd = values.cwd ?? process.cwd();
  const json = values.json ?? false;
  const extraArgs: Record<string, unknown> = {};
  if (values.change) extraArgs.changeId = values.change;
  if (values.timeout) extraArgs.timeout = Number(values.timeout);
  if (values["non-interactive"]) extraArgs.nonInteractive = true;
  if (values.force) extraArgs.force = true;
  if (values.verbose) extraArgs.verbose = true;
  if (values.agent) extraArgs.agent = values.agent;
  if (values.structurePolicy)
    extraArgs.structurePolicy = values.structurePolicy;
  if (values.host) extraArgs.host = values.host;
  if (values.intent) extraArgs.intent = values.intent;
  if (values.answers) {
    try {
      extraArgs.answers = JSON.parse(values.answers);
    } catch {
      /* ignore */
    }
  }

  let result: CommandResult;

  switch (command) {
    case "init":
      result = await runInit(core, cwd, extraArgs, undefined);
      break;
    case "status":
      if (values.loop) extraArgs.loop = true;
      result = await runStatus(core, cwd, extraArgs, undefined);
      break;
    case "new": {
      const requirement = positionals.slice(1).join(" ") || "";
      result = await runNew(core, cwd, requirement, extraArgs, undefined);
      break;
    }
    case "design":
      result = await runDesign(core, cwd, extraArgs, undefined);
      break;
    case "plan":
      result = await runPlan(core, cwd, extraArgs, undefined);
      break;
    case "build": {
      const buildPositionals = positionals.slice(1);
      const subcommand = buildPositionals[0];
      result = await runBuild(
        core,
        cwd,
        subcommand,
        values.task,
        values.result,
        extraArgs,
        undefined,
      );
      break;
    }
    case "verify":
      result = await runVerify(core, cwd, extraArgs, undefined);
      break;
    case "review":
      result = await runReview(core, cwd, extraArgs, undefined);
      break;
    case "archive":
      result = await runArchive(core, cwd, extraArgs, undefined);
      break;
    case "auto": {
      if (values.resume) extraArgs.resume = values.run ?? true;
      if (values.restart) extraArgs.restart = true;
      if (values.stop) extraArgs.stop = true;
      if (values.events) {
        extraArgs.events = true;
        if (values.tail) extraArgs.tail = Number(values.tail);
      }
      if (values["loop-status"]) extraArgs.loopStatus = true;
      const requirement = positionals.slice(1).join(" ") || "";
      result = await runAuto(core, cwd, requirement, extraArgs, undefined);
      break;
    }
    case "codebase": {
      const codebasePositionals = positionals.slice(1);
      const [subcommand, ...queryParts] = codebasePositionals;
      const query = queryParts.join(" ");
      if (query) extraArgs.query = query;
      if (!extraArgs.intent && values.intent) extraArgs.intent = values.intent;
      result = await runCodebase(core, cwd, subcommand, extraArgs, undefined);
      break;
    }
    default:
      // 理论上不会到达（COMMANDS 列表已预检）
      process.exit(ExitCode.GENERAL_ERROR);
  }

  if (json) outputJson(result);
  else outputText(result);

  process.exit(result.exitCode);
}

main().catch((err) => {
  console.error("CLI 内部错误:", (err as Error).message);
  process.exit(ExitCode.GENERAL_ERROR);
});
