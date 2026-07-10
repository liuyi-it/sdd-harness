import { type CommandResult } from "../contracts.js";
/**
 * verify 阶段关注“需求是否被任务覆盖，任务是否有通过的执行证据”。
 * 它不检查实现细节优雅与否，只判断是否达到可验证完成状态。
 */
export declare function runVerify(root: string, args?: Record<string, unknown>, signal?: AbortSignal): Promise<CommandResult>;
//# sourceMappingURL=verify.d.ts.map