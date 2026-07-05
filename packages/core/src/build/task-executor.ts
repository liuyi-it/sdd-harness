import { SddError } from "../errors.js";
import { type TaskDefinition } from "../engines/tdd/tdd-engine.js";

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
  root: string;
  task: TaskDefinition;
  contextPack: string;
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

export interface TaskExecutor {
  execute(request: TaskExecutionRequest): Promise<TaskExecutionResult>;
}

export class MissingTaskExecutor implements TaskExecutor {
  async execute(): Promise<TaskExecutionResult> {
    throw new SddError(
      "E_COMPONENT_UNAVAILABLE",
      "宿主适配器必须为 sdd build 提供 TaskExecutor",
    );
  }
}
