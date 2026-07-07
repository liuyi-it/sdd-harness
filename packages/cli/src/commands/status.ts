import type { SddCore, CommandResult } from "@sdd-harness/core";

export async function runStatus(
  core: SddCore,
  cwd: string,
  signal?: AbortSignal,
): Promise<CommandResult> {
  const request: Parameters<SddCore["execute"]>[0] = {
    command: "status",
    cwd,
    args: {},
  };
  if (signal) request.signal = signal;
  return core.execute(request);
}
