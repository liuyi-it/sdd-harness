import type { TaskExecutionResult } from "../build/task-executor.js";
import type { TaskDefinition } from "../engines/tdd/tdd-engine.js";
export declare function taskEvidenceFailures(task: TaskDefinition, rawResult: TaskExecutionResult): string[];
export declare function tddChainFailures(tasks: TaskDefinition[], results: Array<TaskExecutionResult & {
    taskId: string;
}>): string[];
//# sourceMappingURL=tdd-evidence.d.ts.map