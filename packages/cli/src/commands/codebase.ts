import type { SddCore, CommandResult } from "@sdd-harness/core";

/** sdd codebase 子命令分发 */
export async function runCodebase(
  core: SddCore,
  cwd: string,
  subcommand: string | undefined,
  args: Record<string, unknown>,
  signal?: AbortSignal,
): Promise<CommandResult> {
  const validSubcommands = ["status", "doctor", "index", "query", "rebuild"];

  if (!subcommand || !validSubcommands.includes(subcommand)) {
    return {
      ok: false,
      state: "FAILED",
      exitCode: 2,
      error: {
        code: "E_INVALID_PHASE_COMMAND",
        message: `codebase 子命令必须为 ${validSubcommands.join(" / ")}，当前: ${subcommand ?? "(空)"}`,
        next: "sdd codebase status",
      },
    };
  }

  // 委托给 Core（Core 内部调用 CodebaseMemoryManager）
  const request: Parameters<SddCore["execute"]>[0] = {
    command: "status",
    cwd,
    args: { ...args, codebaseSubcommand: subcommand },
  };
  if (signal) request.signal = signal;
  return core.execute(request);
}
