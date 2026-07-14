import { type CommandResult } from "../contracts.js";
import type { TddEngine } from "../engines/tdd/tdd-engine.js";
/**
 * plan 阶段把设计稿进一步拆成任务、测试计划和上下文摘要。
 * 这里也是后续 build 阶段“允许改哪些文件”的主要事实来源。
 */
export declare function runPlan(root: string, engine: TddEngine, args?: Record<string, unknown>, signal?: AbortSignal): Promise<CommandResult>;
//# sourceMappingURL=plan.d.ts.map