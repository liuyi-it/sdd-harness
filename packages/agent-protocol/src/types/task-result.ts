/** Agent 任务执行结果 — Agent 完成构建任务后写入 */
/** Agent 任务结果状态 — 与 Core v2 协议对齐 */
export type AgentTaskStatus =
  | "SUCCEEDED"
  | "FAILED"
  | "BLOCKED"
  | "SKIPPED"
  | "DEGRADED";

export interface AgentTaskResult {
  schemaVersion: "1.2.0";
  taskId: string;
  status: AgentTaskStatus;
  modifiedFiles: string[];
  createdFiles: string[];
  commandsRun: AgentCommandRun[];
  tddEvidence: AgentTddEvidence[];
  verification: AgentVerification[];
  notes: string[];
}

export interface AgentCommandRun {
  command: string;
  args: string[];
  exitCode: number;
  passed: boolean;
  expectedFailure?: boolean;
  outputSummary: string;
}

export interface AgentTddEvidence {
  phase: "RED" | "GREEN" | "REFACTOR";
  command: string;
  args: string[];
  passed: boolean;
  expectedFailure?: boolean;
  outputSummary: string;
}

export interface AgentVerification {
  command: string;
  args: string[];
  passed: boolean;
  outputSummary: string;
}
