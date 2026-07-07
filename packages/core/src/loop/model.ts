export interface LoopSpec {
  schemaVersion: "1.2.0";
  loopId: string;
  mode: "auto";
  maxSteps: number;
  stoppingRules: string[];
  createdAt: string;
  updatedAt: string;
}

export interface ActiveLoop {
  loopId: string;
  runId: string;
  status: "RUNNING" | "PAUSED" | "FAILED" | "SUCCEEDED" | "ABORTED";
  recovered?: boolean;
}

export interface LoopStep {
  step: number;
  command: string;
  status: "SUCCEEDED" | "FAILED" | "BLOCKED" | "SKIPPED" | "PAUSED";
  startedAt: string;
  endedAt: string;
}

export interface LoopRun {
  schemaVersion: "1.2.0";
  runId: string;
  loopId: string;
  status:
    | "PENDING"
    | "RUNNING"
    | "PAUSED"
    | "SUCCEEDED"
    | "FAILED"
    | "ABORTED"
    | "ARCHIVED";
  startedAt: string;
  endedAt?: string;
  steps: LoopStep[];
}
