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
  minimality?: MinimalityEvidence;
}

/** Agent 对复用、依赖和有意限制的结构化说明；Core 仍以 Git 与 manifest 为事实源。 */
export interface MinimalityEvidence {
  reusedExisting: string[];
  standardLibraryChoices: string[];
  nativePlatformChoices: string[];
  dependenciesAdded: DependencyDecision[];
  abstractionsAdded: AbstractionDecision[];
  deliberateDebts: DeliberateDebtDeclaration[];
}

export interface DependencyDecision {
  name: string;
  manifest: string;
  reason: string;
  requiredBy: string[];
}

export interface AbstractionDecision {
  name: string;
  file: string;
  consumers: string[];
  reason: string;
}

export interface DeliberateDebtDeclaration {
  file: string;
  ceiling: string;
  trigger: string;
  upgrade: string;
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
