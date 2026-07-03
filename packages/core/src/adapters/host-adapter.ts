import {
  COMMANDS,
  type CommandName,
  type CommandResult,
  type SddCore,
} from "../contracts.js";
import { SddError } from "../errors.js";

/**
 * HostAdapter 把 Claude Code / Codex 的命令字符串解析成统一的 Core 请求。
 * 适配器层不保存工作流状态，也不改写 Core 的语义结果。
 */
export type HostStyle = "claude-code" | "codex";

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
  ) {}

  async execute(input: string, cwd: string): Promise<CommandResult> {
    const parsed = parseHostCommand(input, cwd, this.style);
    if (parsed.help) {
      // help/version 由适配器直接返回，避免为了查看帮助信息进入写流程。
      return {
        ok: true,
        state: "NOT_INITIALIZED",
        exitCode: 0,
        data: {
          command: parsed.command,
          usage:
            this.style === "claude-code"
              ? `/sdd.${parsed.command} [options]`
              : `sdd ${parsed.command} [options]`,
          options: [
            "--json",
            "--non-interactive",
            "--force",
            "--timeout",
            "--change",
            "--verbose",
          ],
        },
      };
    }
    return this.core.execute({
      command: parsed.command,
      cwd,
      ...(Object.keys(parsed.args).length === 0 ? {} : { args: parsed.args }),
    });
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
}

export function parseHostCommand(
  input: string,
  cwd: string,
  style: HostStyle,
): ParsedCommand {
  void cwd;
  const tokens = tokenize(input);
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
        throw new SddError(
          "E_INVALID_PHASE_COMMAND",
          `未知的选项：${token ?? ""}`,
        );
    }
  }
  return { command, args, help };
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
