/**
 * 统一校验显式传入的 changeId 是否与当前活动变更一致。
 * 阶段命令不允许悄悄忽略 `--change`，避免用户误操作到错误的变更目录。
 */
export declare function requireActiveChangeId(activeChangeId: string | null, args: Record<string, unknown> | undefined): string;
export declare function assertChangeWritable(root: string, changeId: string): Promise<void>;
//# sourceMappingURL=change-id.d.ts.map