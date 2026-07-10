import type { SddCore, CommandResult } from "@sdd-harness/core";
export declare function runBuild(core: SddCore, cwd: string, subcommand: string | undefined, taskId: string | undefined, resultPath: string | undefined, args: Record<string, unknown>, signal?: AbortSignal): Promise<CommandResult>;
//# sourceMappingURL=build.d.ts.map