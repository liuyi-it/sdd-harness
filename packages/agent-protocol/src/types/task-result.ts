/** Agent 任务执行结果 — Agent 完成构建任务后写入 */
export interface AgentTaskResult {
  schemaVersion: "1.0.0";
  taskId: string;
  status: "DONE" | "FAILED" | "SKIPPED";
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
