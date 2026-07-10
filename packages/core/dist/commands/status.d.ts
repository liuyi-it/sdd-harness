import { type CommandResult } from "../contracts.js";
/**
 * status 是唯一纯只读的公共命令：
 * - 未初始化时返回 NOT_INITIALIZED
 * - 已初始化时原样回报当前持久化状态和建议下一步命令
 */
export declare function runStatus(root: string, args?: Record<string, unknown>): Promise<CommandResult>;
//# sourceMappingURL=status.d.ts.map