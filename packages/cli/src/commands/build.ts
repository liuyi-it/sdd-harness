import type { SddCore, CommandResult } from "@sdd-harness/core";
import fs from "node:fs/promises";

export async function runBuild(
  core: SddCore,
  cwd: string,
  subcommand: string | undefined,
  taskId: string | undefined,
  resultPath: string | undefined,
  args: Record<string, unknown>,
  signal?: AbortSignal,
): Promise<CommandResult> {
  if (subcommand === "next") {
    const request: Parameters<SddCore["execute"]>[0] = {
      command: "build",
      cwd,
      args: { ...args, subcommand: "next" },
    };
    if (signal) request.signal = signal;
    return core.execute(request);
  }

  if (subcommand === "complete") {
    if (!taskId || !resultPath) {
      return {
        ok: false,
        state: "FAILED",
        exitCode: 2,
        error: {
          code: "E_INVALID_PHASE_COMMAND",
          message: "build complete 需要 --task 和 --result 参数",
        },
      };
    }
    let resultJson: unknown;
    try {
      const raw = await fs.readFile(resultPath, "utf-8");
      resultJson = JSON.parse(raw);
    } catch {
      return {
        ok: false,
        state: "FAILED",
        exitCode: 4,
        error: {
          code: "E_MISSING_ARTIFACT",
          message: `无法读取或解析结果文件: ${resultPath}`,
        },
      };
    }
    const request: Parameters<SddCore["execute"]>[0] = {
      command: "build",
      cwd,
      args: { ...args, subcommand: "complete", taskId, result: resultJson },
    };
    if (signal) request.signal = signal;
    return core.execute(request);
  }

  // 无子命令：返回 build 状态
  const request: Parameters<SddCore["execute"]>[0] = {
    command: "build",
    cwd,
    args,
  };
  if (signal) request.signal = signal;
  return core.execute(request);
}
