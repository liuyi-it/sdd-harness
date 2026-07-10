import type { SddCore, CommandResult } from "@sdd-harness/core";
/**
 * CLI 层 init 命令：
 * 1. 如果指定了 --agent，校验是否都在可用列表中，不在则报错
 * 2. 如果未指定 --agent，进入交互式选择
 * 3. --non-interactive 且无 --agent 时报错
 * 4. 将选中的 agent 列表写入 args.agent 后调用 Core
 */
export declare function runInit(core: SddCore, cwd: string, args: Record<string, unknown>, signal?: AbortSignal): Promise<CommandResult>;
//# sourceMappingURL=init.d.ts.map