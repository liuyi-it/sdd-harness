import type { SddCore, CommandResult } from "@sdd-harness/core";

export async function runNew(
  core: SddCore,
  cwd: string,
  requirement: string,
  args: Record<string, unknown>,
  signal?: AbortSignal,
): Promise<CommandResult> {
  const request: Parameters<SddCore["execute"]>[0] = {
    command: "new",
    cwd,
    args: { ...args, requirement },
  };
  if (signal) request.signal = signal;
  return core.execute(request);
}
