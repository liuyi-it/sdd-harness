import type { SddCore, CommandResult } from "@sdd-harness/core";
/** sdd codebase 子命令分发 */
export declare function runCodebase(core: SddCore, cwd: string, subcommand: string | undefined, args: Record<string, unknown>, signal?: AbortSignal): Promise<CommandResult>;
//# sourceMappingURL=codebase.d.ts.map