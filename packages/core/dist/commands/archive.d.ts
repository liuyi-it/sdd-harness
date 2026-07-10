import { type CommandResult } from "../contracts.js";
/**
 * archive 阶段把整个 change 固化为只读归档：
 * - 生成 traceability 和 archive-report
 * - 记录归档摘要
 * - 将状态推进到 ARCHIVED
 */
export declare function runArchive(root: string, args?: Record<string, unknown>, signal?: AbortSignal): Promise<CommandResult>;
//# sourceMappingURL=archive.d.ts.map