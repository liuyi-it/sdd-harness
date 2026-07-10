import type { CommandResult } from "../contracts.js";
import type { LoopDecision } from "./model.js";
/**
 * DecisionEngine：纯函数，根据 CommandResult 决策下一步动作。
 * 规则见 docs/四期需求文档.md §11.2
 */
export declare function decide(input: {
    result: CommandResult;
}): LoopDecision;
//# sourceMappingURL=loop-decision.d.ts.map