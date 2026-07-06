import { SddError } from "../errors.js";
import type { GitSnapshot } from "../git/git-inspector.js";
import { type TaskDefinition } from "../engines/tdd/tdd-engine.js";
import type { ProjectRuleSnapshot } from "../project-conventions/rule-resolver.js";

export interface VerificationEvidence {
  command: string;
  passed: boolean;
  output: string;
}

export interface TddEvidence {
  phase: "RED" | "GREEN" | "REFACTOR" | "VERIFY";
  command: string;
  passed: boolean;
  expectedFailure?: boolean;
  output: string;
}

export interface TaskExecutionRequest {
  schemaVersion: "1.2.0";
  root: string;
  changeId: string;
  runId: string;
  task: TaskDefinition;
  contextPack: string;
  gitBaseline: GitSnapshot | null;
  constraints: {
    allowedFiles: string[];
    expectedNewFiles: string[];
    forbiddenFiles: string[];
    allowedCommands: string[];
  };
  mode: "subagent" | "main-agent";
  projectRules?: ProjectRuleSnapshot;
  signal?: AbortSignal;
}

/**
 * TaskExecutor 抽象“谁真正去执行任务实现”。
 * Core 只约束输入输出契约，不直接绑定具体 AI 或 shell 执行方式。
 */
export interface TaskExecutionResult {
  modifiedFiles: string[];
  tddEvidence: TddEvidence[];
  verification: VerificationEvidence[];
}

export interface AllowedCommand {
  command: string;
  args: string[];
}

export interface TaskCommandEvidence extends AllowedCommand {
  exitCode?: number;
  outputSummary: string;
}

export interface TaskFileDelta {
  added: string[];
  modified: string[];
  deleted: string[];
}

export interface TaskExecutionResultV2 {
  schemaVersion: "1.2.0";
  taskId?: string;
  status: "SUCCEEDED" | "FAILED" | "BLOCKED" | "SKIPPED" | "DEGRADED";
  summary: string;
  commandEvidence: TaskCommandEvidence[];
  fileDelta: TaskFileDelta;
  timestamps: {
    startedAt: string;
    endedAt: string;
  };
  mode?: {
    requested: "subagent" | "main-agent";
    actual: "subagent" | "main-agent";
  };
  notes?: string[];
  legacy?: TaskExecutionResult;
}

export type TaskExecutionOutput = TaskExecutionResult | TaskExecutionResultV2;

export interface TaskExecutor {
  execute(request: TaskExecutionRequest): Promise<TaskExecutionOutput>;
}

export class MissingTaskExecutor implements TaskExecutor {
  async execute(): Promise<TaskExecutionOutput> {
    throw new SddError(
      "E_COMPONENT_UNAVAILABLE",
      "宿主适配器必须为 sdd build 提供 TaskExecutor",
    );
  }
}
