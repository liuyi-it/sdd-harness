import { type TaskExecutionOutput, type TaskExecutionResult, type TaskExecutionResultV2, type TaskFileDelta } from "./task-executor.js";
export interface NormalizedTaskExecutionArtifact extends TaskExecutionResultV2 {
    taskId: string;
    mode: {
        requested: "subagent" | "main-agent";
        actual: "subagent" | "main-agent";
    };
}
interface NormalizeOptions {
    actualFileDelta: TaskFileDelta;
    startedAt: string;
    endedAt: string;
    requestedMode: "subagent" | "main-agent";
    actualMode: "subagent" | "main-agent";
    degradedReason?: string;
}
export declare function normalizeTaskExecutionResult(raw: TaskExecutionOutput | (TaskExecutionResult & {
    taskId: string;
}), options: NormalizeOptions): NormalizedTaskExecutionArtifact;
export {};
//# sourceMappingURL=task-result-normalizer.d.ts.map