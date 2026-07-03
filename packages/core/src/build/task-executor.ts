import { SddError } from "../errors.js";
import { type TaskDefinition } from "../engines/tdd/tdd-engine.js";

export interface VerificationEvidence {
  command: string;
  passed: boolean;
  output: string;
}

export interface TaskExecutionRequest {
  root: string;
  task: TaskDefinition;
  contextPack: string;
  signal?: AbortSignal;
}

export interface TaskExecutionResult {
  modifiedFiles: string[];
  verification: VerificationEvidence[];
}

export interface TaskExecutor {
  execute(request: TaskExecutionRequest): Promise<TaskExecutionResult>;
}

export class MissingTaskExecutor implements TaskExecutor {
  async execute(): Promise<TaskExecutionResult> {
    throw new SddError(
      "E_COMPONENT_UNAVAILABLE",
      "The host adapter must provide a TaskExecutor for sdd build",
    );
  }
}
