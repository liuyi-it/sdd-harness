import {
  COMMANDS,
  ERROR_EXIT_CODES,
  type CommandName,
  type CommandResult,
  type SddCore,
} from "../contracts.js";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { SddError } from "../errors.js";

/**
 * HostAdapter 把 Claude Code / Codex 的命令字符串解析成统一的 Core 请求。
 * 适配器层不保存工作流状态，也不改写 Core 的语义结果。
 */
export type HostStyle = "claude-code" | "codex";
export type HostOutputMode = "collaborative" | "strict-audit" | "diagnostic";

export interface HostAdapterOptions {
  /** 默认协作模式只输出用户需要决策的信息；审计和诊断模式保留协议字段。 */
  outputMode?: HostOutputMode;
}

export interface PluginVersion {
  name: "sdd-harness";
  version: string;
  delivery: "plugin";
  supportedTargets: ["claude-code", "codex"];
}

export class HostAdapter {
  constructor(
    private readonly core: SddCore,
    private readonly style: HostStyle,
    private readonly options: HostAdapterOptions = {},
  ) {}

  async execute(input: string, cwd: string): Promise<CommandResult> {
    const parsed = parseHostCommand(input, cwd, this.style);
    if (parsed.version) {
      return renderResult(
        {
          ok: true,
          state: "NOT_INITIALIZED",
          exitCode: 0,
          data: this.version(),
        },
        parsed.args.json === true,
        this.outputMode(),
      );
    }
    if (parsed.help) {
      // help/version 由适配器直接返回，避免为了查看帮助信息进入写流程。
      return renderResult(
        {
          ok: true,
          state: "NOT_INITIALIZED",
          exitCode: 0,
          data: helpDocument(parsed.command, this.style),
        },
        parsed.args.json === true,
        this.outputMode(),
      );
    }
    return renderResult(
      await this.executeCore(parsed, cwd),
      parsed.args.json === true,
      this.outputMode(),
    );
  }

  private async executeCore(
    parsed: ParsedCommand,
    cwd: string,
  ): Promise<CommandResult> {
    const args = { ...parsed.args };
    if (
      parsed.command === "build" &&
      args.subcommand === "complete" &&
      typeof args.resultFile === "string"
    ) {
      try {
        args.result = JSON.parse(
          await readFile(resolve(cwd, args.resultFile), "utf8"),
        );
      } catch {
        throw new SddError("E_MISSING_ARTIFACT", "无法读取或解析任务执行结果");
      }
      delete args.resultFile;
    }
    return this.core.execute({
      command: parsed.command,
      cwd,
      ...(Object.keys(args).length === 0 ? {} : { args }),
    });
  }

  private outputMode(): HostOutputMode {
    return this.options.outputMode ?? "collaborative";
  }

  version(): PluginVersion {
    return {
      name: "sdd-harness",
      version: "0.1.0",
      delivery: "plugin",
      supportedTargets: ["claude-code", "codex"],
    };
  }
}

interface ParsedCommand {
  command: CommandName;
  args: Record<string, unknown>;
  help: boolean;
  version: boolean;
}

interface HelpDocument {
  command: CommandName;
  description: string;
  usage: string;
  options: Array<{
    name: string;
    type: string;
    default: string;
    description: string;
  }>;
  examples: string[];
  exitCodes: Array<{ code: number; meaning: string }>;
}

export function parseHostCommand(
  input: string,
  cwd: string,
  style: HostStyle,
): ParsedCommand {
  void cwd;
  const tokens = tokenize(input);
  if (style === "claude-code" && tokens[0] === "/sdd.version") {
    return { command: "status", args: {}, help: false, version: true };
  }
  if (style === "codex" && tokens[0] === "sdd" && tokens[1] === "--version") {
    return { command: "status", args: {}, help: false, version: true };
  }
  const rawCommand =
    style === "claude-code"
      ? tokens.shift()?.replace(/^\/sdd\./, "")
      : parseCodexPrefix(tokens);
  if (
    rawCommand === undefined ||
    !(COMMANDS as readonly string[]).includes(rawCommand)
  ) {
    throw new SddError(
      "E_INVALID_PHASE_COMMAND",
      `未知的 sdd 命令：${rawCommand ?? ""}`,
    );
  }
  const command = rawCommand as CommandName;
  const args: Record<string, unknown> = {};
  if (
    (command === "build" || command === "codebase") &&
    tokens[0]?.startsWith("--") === false
  ) {
    args.subcommand = tokens.shift();
  }
  // new/auto 允许第一个非选项参数直接作为自然语言需求输入。
  if (
    (command === "new" || command === "auto") &&
    tokens[0]?.startsWith("--") === false
  ) {
    args.requirement = tokens.shift();
  }
  let help = false;
  while (tokens.length > 0) {
    const token = tokens.shift();
    switch (token) {
      case "--help":
        help = true;
        break;
      case "--json":
        args.json = true;
        break;
      case "--non-interactive":
        args.nonInteractive = true;
        break;
      case "--force":
        args.force = true;
        break;
      case "--verbose":
        args.verbose = true;
        break;
      case "--change":
        args.changeId = requireValue(tokens, token);
        break;
      case "--answers":
        args.answers = parseAnswers(requireValue(tokens, token));
        break;
      case "--task":
        args.taskId = requireValue(tokens, token);
        break;
      case "--result":
        args.resultFile = requireValue(tokens, token);
        break;
      case "--intent":
        args.intent = requireValue(tokens, token);
        break;
      case "--agent":
        args.agent = requireValue(tokens, token);
        break;
      case "--host":
        args.host = requireValue(tokens, token);
        break;
      case "--structure-policy":
        args.structurePolicy = requireValue(tokens, token);
        break;
      case "--resume":
        args.resume = true;
        break;
      case "--restart":
        args.restart = true;
        break;
      case "--stop":
        args.stop = true;
        break;
      case "--events":
        args.events = true;
        break;
      case "--loop-status":
        args.loopStatus = true;
        break;
      case "--loop":
        args.loop = true;
        break;
      case "--run":
        args.resume = requireValue(tokens, token);
        break;
      case "--tail": {
        const value = Number(requireValue(tokens, token));
        if (!Number.isInteger(value) || value < 0)
          throw new SddError(
            "E_INVALID_PHASE_COMMAND",
            "--tail 必须是非负整数",
          );
        args.tail = value;
        break;
      }
      case "--timeout": {
        const value = Number(requireValue(tokens, token));
        if (!Number.isFinite(value) || value < 0)
          throw new SddError(
            "E_INVALID_PHASE_COMMAND",
            "--timeout 必须是非负数",
          );
        args.timeout = value;
        break;
      }
      default:
        if (command === "codebase" && args.subcommand === "query") {
          args.query = [token, ...tokens.splice(0)].join(" ");
          break;
        }
        throw new SddError(
          "E_INVALID_PHASE_COMMAND",
          `未知的选项：${token ?? ""}`,
        );
    }
  }
  return { command, args, help, version: false };
}

function parseCodexPrefix(tokens: string[]): string | undefined {
  if (tokens.shift() !== "sdd") return undefined;
  return tokens.shift();
}

function tokenize(input: string): string[] {
  const tokens: string[] = [];
  const expression = /"([^"]*)"|'([^']*)'|(\S+)/g;
  for (const match of input.matchAll(expression)) {
    const token = match[1] ?? match[2] ?? match[3];
    if (token !== undefined) tokens.push(token);
  }
  return tokens;
}

function requireValue(tokens: string[], option: string): string {
  const value = tokens.shift();
  if (value === undefined || value.startsWith("--")) {
    throw new SddError("E_INVALID_PHASE_COMMAND", `${option} 需要提供一个值`);
  }
  return value;
}

function parseAnswers(value: string): Record<string, string> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    throw new SddError("E_INVALID_PHASE_COMMAND", "--answers 必须是 JSON 对象");
  }
  if (
    parsed === null ||
    Array.isArray(parsed) ||
    typeof parsed !== "object" ||
    !Object.values(parsed).every((answer) => typeof answer === "string")
  ) {
    throw new SddError(
      "E_INVALID_PHASE_COMMAND",
      "--answers 必须是字符串答案组成的 JSON 对象",
    );
  }
  return parsed as Record<string, string>;
}

function helpDocument(command: CommandName, style: HostStyle): HelpDocument {
  const usage = formatCommand(style, command);
  return {
    command,
    description: COMMAND_DESCRIPTIONS[command],
    usage: `${usage} [options]`,
    options: [
      {
        name: "--json",
        type: "boolean",
        default: "false",
        description: "输出机器可读 JSON",
      },
      {
        name: "--non-interactive",
        type: "boolean",
        default: "false",
        description: "禁止交互提问；遇到 BLOCKER 直接失败",
      },
      {
        name: "--force",
        type: "boolean",
        default: "false",
        description: "强制覆盖受管制品或集成文件，不执行就地合并",
      },
      {
        name: "--timeout <seconds>",
        type: "number",
        default: "0",
        description: "0 表示不主动超时；写命令会在锁等待和阶段执行时应用超时",
      },
      {
        name: "--change <id>",
        type: "string",
        default: "current",
        description: "指定 Change ID；new/auto 可显式传入",
      },
      {
        name: "--verbose",
        type: "boolean",
        default: "false",
        description: "输出调试信息",
      },
    ],
    examples: COMMAND_EXAMPLES[command].map((example) =>
      style === "claude-code" ? example.replace(/^sdd /, "/sdd.") : example,
    ),
    exitCodes: HELP_EXIT_CODES[command],
  };
}

function renderResult(
  result: CommandResult,
  asJson: boolean,
  outputMode: HostOutputMode,
): CommandResult {
  return {
    ...result,
    rendered: asJson
      ? {
          format: "json",
          content: `${JSON.stringify(jsonView(result), null, 2)}\n`,
        }
      : {
          format: "text",
          content:
            outputMode === "collaborative"
              ? collaborativeView(result)
              : auditTextView(result),
        },
  };
}

function jsonView(result: CommandResult): Record<string, unknown> {
  return {
    ok: result.ok,
    state: result.state,
    exitCode: result.exitCode,
    ...(result.changeId === undefined ? {} : { changeId: result.changeId }),
    ...(result.next === undefined ? {} : { next: result.next }),
    ...(result.data === undefined ? {} : { data: result.data }),
    ...(result.warnings === undefined ? {} : { warnings: result.warnings }),
    ...(result.error === undefined ? {} : { error: result.error }),
  };
}

function collaborativeView(result: CommandResult): string {
  if (!result.ok) return `暂时无法继续：${userFacingError(result)}\n`;

  if (result.state === "CLARIFYING") {
    const questions = clarificationQuestions(result.data);
    const lines = ["我需要先确认以下信息：", ...questions.map((q) => `- ${q}`)];
    if (questions.length === 0)
      lines.push("- 请补充完成这项需求所需的业务规则。");
    return `${lines.join("\n")}\n`;
  }
  if (result.state === "BUILD_WAITING_AGENT")
    return "已理解并推进：正在按既定范围实施并验证。\n";
  if (result.state === "ARCHIVED")
    return "已完成：变更、验证、审查和归档记录均已保留。\n";
  if (result.state === "PAUSED") return "当前已暂停，等待你的下一步决定。\n";
  if (result.state === "FAILED")
    return "当前受阻：执行未完成，需要先处理已有问题。\n";
  return result.warnings !== undefined && result.warnings.length > 0
    ? "已理解并推进：当前步骤已完成，但仍有需要留意的风险。\n"
    : "已理解并推进：当前步骤已完成，正在继续后续工作。\n";
}

function clarificationQuestions(data: unknown): string[] {
  if (!isRecord(data) || !isRecord(data.clarification)) return [];
  const questions = data.clarification.questions;
  if (!Array.isArray(questions)) return [];
  return questions.flatMap((item) =>
    isRecord(item) && typeof item.question === "string" ? [item.question] : [],
  );
}

function userFacingError(result: CommandResult): string {
  switch (result.error?.code) {
    case "E_UNRESOLVED_BLOCKER":
      return "需求还有需要确认的业务信息。";
    case "E_CONCURRENT_RUN":
    case "E_LOCK_TIMEOUT":
      return "已有操作正在进行，请稍后再试。";
    case "E_VERIFY_FAILED":
    case "E_TDD_EVIDENCE_REQUIRED":
    case "E_AGENT_TASK_FAILED":
      return "验证未通过，需要先修复实现或测试问题。";
    case "E_SECURITY_BLOCKED":
    case "E_PATH_OUTSIDE_REPO":
    case "E_SYMLINK_BLOCKED":
      return "该操作触及受保护范围，需要调整方案或明确授权。";
    default:
      return "工作流暂时无法继续；如需排查详情，请切换到诊断模式。";
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function auditTextView(result: CommandResult): string {
  const lines = [`SDD Status: ${result.state}`];
  if (result.changeId !== undefined) {
    lines.push("", "Change:", result.changeId);
  }
  if (result.next !== undefined) {
    lines.push("", "Next:", result.next);
  }
  if (result.error !== undefined) {
    lines.push("", `Error: ${result.error.code}`, result.error.message);
    if (result.error.next !== undefined) {
      lines.push(`Next: ${result.error.next}`);
    }
  }
  if (result.warnings !== undefined && result.warnings.length > 0) {
    lines.push(
      "",
      "Warnings:",
      ...result.warnings.map((item) =>
        typeof item === "string"
          ? `- ${item}`
          : `- [${item.code}] ${item.message}`,
      ),
    );
  }
  return `${lines.join("\n")}\n`;
}

function formatCommand(style: HostStyle, command: CommandName): string {
  return style === "claude-code" ? `/sdd.${command}` : `sdd ${command}`;
}

const COMMAND_DESCRIPTIONS: Record<CommandName, string> = {
  init: "初始化 .sdd/、插件集成文件、Schema 和代码库索引摘要。",
  auto: "从当前阶段自动推进后续流程，遇到 BLOCKER 或失败时暂停。",
  new: "创建或继续一个 Change，生成需求澄清与 spec 制品。",
  design: "基于 spec、impact 和索引上下文生成设计稿。",
  plan: "基于 design 生成 tasks、test-plan 和 Context Pack。",
  build: "按任务执行实现，校验文件范围并沉淀任务执行证据。",
  verify: "检查任务完成度、需求覆盖和验收标准覆盖。",
  review: "检查实现证据、文件范围和无关修改风险。",
  archive: "归档当前 Change，生成追溯和归档报告并切换为只读。",
  status: "查看当前初始化状态、阶段、Change 和建议下一步。",
  codebase: "代码库上下文管理：状态、诊断、索引、查询与重建。",
};

const COMMAND_EXAMPLES: Record<CommandName, string[]> = {
  init: ["sdd init", "sdd init --json"],
  auto: ['sdd auto "实现订单取消功能" --change add-order-cancel'],
  new: ['sdd new "实现订单取消功能" --change add-order-cancel'],
  design: ["sdd design", "sdd design --json"],
  plan: ["sdd plan", "sdd plan --json"],
  build: ["sdd build", "sdd build --timeout 600"],
  verify: ["sdd verify", "sdd verify --json"],
  review: ["sdd review", "sdd review --json"],
  archive: ["sdd archive", "sdd archive --json"],
  status: ["sdd status", "sdd status --json"],
  codebase: ["sdd codebase status", "sdd codebase doctor"],
};

const COMMON_EXIT_CODES = [
  { code: 0, meaning: "成功" },
  {
    code: ERROR_EXIT_CODES.E_INVALID_PHASE_COMMAND,
    meaning: "状态非法或命令不可用",
  },
  { code: ERROR_EXIT_CODES.E_MISSING_ARTIFACT, meaning: "缺少必要制品" },
];

const HELP_EXIT_CODES: Record<
  CommandName,
  Array<{ code: number; meaning: string }>
> = {
  init: [
    { code: 0, meaning: "成功或降级模式成功" },
    { code: ERROR_EXIT_CODES.E_CONCURRENT_RUN, meaning: "存在并发写命令" },
    { code: ERROR_EXIT_CODES.E_STATE_CORRUPTED, meaning: "配置或状态文件损坏" },
  ],
  auto: [
    { code: 0, meaning: "成功、暂停于 CLARIFYING，或流程最终归档" },
    { code: ERROR_EXIT_CODES.E_NOT_INITIALIZED, meaning: "项目未初始化" },
    { code: ERROR_EXIT_CODES.E_TIMEOUT, meaning: "阶段执行超时" },
  ],
  new: [
    { code: 0, meaning: "成功或进入 CLARIFYING" },
    { code: ERROR_EXIT_CODES.E_UNRESOLVED_BLOCKER, meaning: "BLOCKER 未回答" },
    {
      code: ERROR_EXIT_CODES.E_ACTIVE_CHANGE_EXISTS,
      meaning: "存在未完成的 active Change",
    },
  ],
  design: COMMON_EXIT_CODES,
  plan: COMMON_EXIT_CODES,
  build: [
    { code: 0, meaning: "成功" },
    {
      code: ERROR_EXIT_CODES.E_TDD_EVIDENCE_REQUIRED,
      meaning: "缺少有效的 TDD 阶段证据",
    },
    { code: ERROR_EXIT_CODES.E_VERIFY_FAILED, meaning: "任务验证失败" },
    {
      code: ERROR_EXIT_CODES.E_SECURITY_BLOCKED,
      meaning: "文件或命令越界被阻断",
    },
    { code: ERROR_EXIT_CODES.E_TIMEOUT, meaning: "任务执行超时" },
    { code: ERROR_EXIT_CODES.E_INTERRUPTED, meaning: "用户中断" },
  ],
  verify: [
    { code: 0, meaning: "成功" },
    {
      code: ERROR_EXIT_CODES.E_VERIFY_FAILED,
      meaning: "需求、任务或验收标准未覆盖",
    },
    ...COMMON_EXIT_CODES.slice(1),
  ],
  review: [
    { code: 0, meaning: "成功" },
    { code: ERROR_EXIT_CODES.E_REVIEW_FAILED, meaning: "审查发现问题" },
    { code: ERROR_EXIT_CODES.E_VERIFY_REQUIRED, meaning: "必须先 verify" },
  ],
  archive: [
    { code: 0, meaning: "成功或 already archived" },
    { code: ERROR_EXIT_CODES.E_REVIEW_REQUIRED, meaning: "必须先 review" },
    {
      code: ERROR_EXIT_CODES.E_MISSING_ARTIFACT,
      meaning: "缺少归档前必要报告",
    },
  ],
  status: [{ code: 0, meaning: "成功" }],
  codebase: COMMON_EXIT_CODES,
};
