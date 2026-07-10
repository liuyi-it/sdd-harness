import { type CommandResult } from "../contracts.js";
/**
 * review 阶段在 verify 通过之后再做一次实现侧审查，
 * 重点确认修改范围、验证证据和任务声明之间没有漂移。
 */
export declare function runReview(root: string, args?: Record<string, unknown>, signal?: AbortSignal): Promise<CommandResult>;
//# sourceMappingURL=review.d.ts.map