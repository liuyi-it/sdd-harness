import type { ErrorCode, Phase } from "../contracts.js";
import { SddError } from "../errors.js";
import type { StateStore, WorkflowState } from "../state/state-store.js";

/**
 * 判断当前命令是否允许在“失败/暂停后恢复”的语义下继续执行。
 * 只有当状态里记录的失败命令或中断命令与当前命令一致时，才允许恢复。
 */
export function canResumeCommand(
  state: {
    currentPhase: string;
    failedCommand: string | null;
    interruptedCommand: string | null;
  },
  command: string,
): boolean {
  return (
    (state.currentPhase === "FAILED" && state.failedCommand === command) ||
    (state.currentPhase === "PAUSED" && state.interruptedCommand === command)
  );
}

const RECOVERY_RULES: Record<
  string,
  { previousPhases: Phase[]; inProgressPhases: Phase[] }
> = {
  "sdd init": {
    previousPhases: ["NOT_INITIALIZED"],
    inProgressPhases: ["INITIALIZING", "INDEXING"],
  },
  "sdd new": {
    previousPhases: ["INDEX_READY", "NEW_STARTED", "CLARIFYING"],
    inProgressPhases: ["NEW_STARTED"],
  },
  "sdd design": {
    previousPhases: ["SPEC_READY", "DESIGNING"],
    inProgressPhases: ["DESIGNING"],
  },
  "sdd plan": {
    previousPhases: ["DESIGN_READY", "PLANNING"],
    inProgressPhases: ["PLANNING"],
  },
  "sdd build": {
    previousPhases: ["PLAN_READY", "BUILDING"],
    inProgressPhases: ["BUILDING"],
  },
  "sdd verify": {
    previousPhases: ["BUILD_READY", "VERIFYING"],
    inProgressPhases: ["VERIFYING"],
  },
  "sdd review": {
    previousPhases: ["VERIFY_READY", "REVIEWING"],
    inProgressPhases: ["REVIEWING"],
  },
  "sdd archive": {
    previousPhases: ["REVIEW_READY", "ARCHIVING"],
    inProgressPhases: ["ARCHIVING"],
  },
  "sdd auto": {
    previousPhases: [
      "INDEX_READY",
      "CLARIFYING",
      "SPEC_READY",
      "DESIGN_READY",
      "PLAN_READY",
      "BUILD_READY",
      "VERIFY_READY",
      "REVIEW_READY",
      "FAILED",
      "PAUSED",
    ],
    inProgressPhases: [
      "NEW_STARTED",
      "DESIGNING",
      "PLANNING",
      "BUILDING",
      "VERIFYING",
      "REVIEWING",
      "ARCHIVING",
    ],
  },
};

export function assertRecoverableCommandState(
  state: Pick<
    WorkflowState,
    | "currentPhase"
    | "previousPhase"
    | "inProgressPhase"
    | "failedCommand"
    | "interruptedCommand"
  >,
  command: string,
): void {
  if (!canResumeCommand(state, command)) return;
  const rule = RECOVERY_RULES[command];
  if (rule === undefined) return;
  if (
    state.previousPhase === null ||
    state.inProgressPhase === null ||
    !rule.previousPhases.includes(state.previousPhase) ||
    !rule.inProgressPhases.includes(state.inProgressPhase)
  ) {
    throw new SddError(
      "E_STATE_CORRUPTED",
      `恢复命令 ${command} 的状态上下文不合法：previousPhase=${state.previousPhase ?? "null"}，inProgressPhase=${state.inProgressPhase ?? "null"}`,
      "sdd status",
    );
  }
}

export function previousStablePhase(
  state: Pick<WorkflowState, "currentPhase" | "previousPhase">,
  fallback: Phase,
): Phase {
  return state.currentPhase === "FAILED" || state.currentPhase === "PAUSED"
    ? (state.previousPhase ?? fallback)
    : state.currentPhase;
}

export function normalizeCommandError(
  error: unknown,
  fallbackCode: ErrorCode,
  next?: string,
): SddError {
  if (error instanceof SddError) return error;
  if (isNodeError(error) && error.code === "ENOENT") {
    return new SddError(
      "E_MISSING_ARTIFACT",
      `缺少必要制品：${error.path ?? error.message}`,
      next,
    );
  }
  if (error instanceof SyntaxError) {
    return new SddError(
      "E_STATE_CORRUPTED",
      `状态或制品解析失败：${error.message}`,
      next,
    );
  }
  if (error instanceof Error) {
    return new SddError(fallbackCode, error.message, next);
  }
  return new SddError(fallbackCode, String(error), next);
}

export async function persistCommandFailure(
  store: StateStore,
  error: SddError,
  options: {
    command: string;
    previousPhase: Phase;
    inProgressPhase: Phase;
  },
): Promise<void> {
  await store.update((current) => ({
    ...current,
    currentPhase: error.exitCode === 130 ? "PAUSED" : "FAILED",
    previousPhase: options.previousPhase,
    inProgressPhase: options.inProgressPhase,
    failedCommand: error.exitCode === 130 ? null : options.command,
    failedReason: error.exitCode === 130 ? null : error.message,
    interruptedCommand: error.exitCode === 130 ? options.command : null,
    recoverable: true,
    lastError: error.code,
    suggestedCommand: options.command,
  }));
}

function isNodeError(
  error: unknown,
): error is Error & { code?: string; path?: string } {
  return typeof error === "object" && error !== null && "message" in error;
}
