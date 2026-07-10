import type { ErrorCode, Phase } from "../contracts.js";
import { SddError } from "../errors.js";
import type { StateStore, WorkflowState } from "../state/state-store.js";
/**
 * 判断当前命令是否允许在“失败/暂停后恢复”的语义下继续执行。
 * 只有当状态里记录的失败命令或中断命令与当前命令一致时，才允许恢复。
 */
export declare function canResumeCommand(state: {
    currentPhase: string;
    failedCommand: string | null;
    interruptedCommand: string | null;
}, command: string): boolean;
export declare function assertRecoverableCommandState(state: Pick<WorkflowState, "currentPhase" | "previousPhase" | "inProgressPhase" | "failedCommand" | "interruptedCommand">, command: string): void;
export declare function previousStablePhase(state: Pick<WorkflowState, "currentPhase" | "previousPhase">, fallback: Phase): Phase;
export declare function normalizeCommandError(error: unknown, fallbackCode: ErrorCode, next?: string): SddError;
export declare function persistCommandFailure(store: StateStore, error: SddError, options: {
    command: string;
    previousPhase: Phase;
    inProgressPhase: Phase;
}): Promise<void>;
//# sourceMappingURL=recovery.d.ts.map