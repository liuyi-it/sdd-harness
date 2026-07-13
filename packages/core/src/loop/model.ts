import type {
  AgentActionRequired,
  CommandError,
  CliWarning,
} from "../contracts.js";

/** Loop 决策类型：根据 CommandResult 和当前 phase 决定下一步动作 */
export type LoopDecision =
  | "CONTINUE"
  | "PAUSE_FOR_AGENT"
  | "PAUSE_FOR_CLARIFICATION"
  | "PAUSE_FOR_HUMAN"
  | "FAIL"
  | "ABORT"
  | "DONE";

/** Loop 策略 */
export type LoopDecisionPolicy = "STRICT" | "BALANCED";

/** Loop 停止规则 */
export type LoopStoppingRule =
  | "CLARIFYING"
  | "WAITING_AGENT"
  | "VERIFY_FAILED"
  | "REVIEW_FAILED"
  | "SECURITY_BLOCKED"
  | "STATE_CORRUPTED";

/** Loop 规范 */
export interface LoopSpec {
  schemaVersion: "1.3.0";
  loopId: string;
  mode: "auto";
  maxSteps: number;
  maxRetriesPerStep: number;
  maxRepeatedFailures: number;
  repairPolicy: {
    maxRepairAttemptsPerTask: number;
    maxRepeatedFailureSignature: number;
    stopOnScopeExpansion: boolean;
  };
  stoppingRules: LoopStoppingRule[];
  decisionPolicy: LoopDecisionPolicy;
  createdAt: string;
  updatedAt: string;
}

/** 活跃 Loop 摘要 */
export interface ActiveLoop {
  loopId: string;
  runId: string;
  status:
    | "RUNNING"
    | "WAITING_AGENT"
    | "PAUSED"
    | "FAILED"
    | "SUCCEEDED"
    | "ABORTED";
  waiting?: LoopWaitingState;
  recovered?: boolean;
}

/** Loop 等待状态详情 */
export interface LoopWaitingState {
  reason: "AGENT_TASK_EXECUTION" | "CLARIFICATION" | "HUMAN_REVIEW";
  taskId?: string;
  resultFile?: string;
  since: string;
}

/** Loop 步骤 */
export interface LoopStep {
  step: number;
  kind:
    | "COMMAND"
    | "AGENT_HANDOFF"
    | "DECISION"
    | "VERIFY"
    | "REVIEW"
    | "ARCHIVE";
  command: string;
  phaseBefore: string;
  phaseAfter?: string;
  status:
    | "SUCCEEDED"
    | "FAILED"
    | "BLOCKED"
    | "SKIPPED"
    | "PAUSED"
    | "WAITING_AGENT";
  decision?: LoopDecision;
  actionRequired?: AgentActionRequired;
  error?: CommandError;
  warnings?: Array<string | CliWarning>;
  artifacts?: string[];
  startedAt: string;
  endedAt: string;
}

/** Loop Run 记录 */
export interface LoopRun {
  schemaVersion: "1.3.0";
  runId: string;
  loopId: string;
  changeId?: string;
  status:
    | "PENDING"
    | "RUNNING"
    | "WAITING_AGENT"
    | "PAUSED"
    | "SUCCEEDED"
    | "FAILED"
    | "ABORTED"
    | "ARCHIVED";
  startedAt: string;
  updatedAt: string;
  endedAt?: string;
  currentStep: number;
  lastDecision?: LoopDecision;
  waiting?: LoopWaitingState;
  steps: LoopStep[];
}
