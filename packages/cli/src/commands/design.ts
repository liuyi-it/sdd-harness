import type { SddCore, CommandResult } from "@sdd-harness/core";

export async function runDesign(
  core: SddCore,
  cwd: string,
  args: Record<string, unknown>,
  signal?: AbortSignal,
): Promise<CommandResult> {
  const request: Parameters<SddCore["execute"]>[0] = {
    command: "design",
    cwd,
    args,
  };
  if (signal) request.signal = signal;
  return core.execute(request);
}
