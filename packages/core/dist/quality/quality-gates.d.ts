import { type TaskExecutionResult } from "../build/task-executor.js";
import { type TaskDefinition } from "../engines/tdd/tdd-engine.js";
import { type GitSnapshot } from "../git/git-inspector.js";
import type { SpecDocument } from "../engines/openspec/model.js";
/**
 * verify / review 阶段共享的质量闸门检查逻辑。
 * 保持纯函数接口，便于命令层组合和测试层断言。
 */
export interface StoredTaskResult extends TaskExecutionResult {
    taskId: string;
}
export interface GateResult {
    passed: boolean;
    failures: string[];
}
export declare function verifyGate(spec: string | SpecDocument, tasks: TaskDefinition[], results: StoredTaskResult[], statuses: Record<string, string>): GateResult;
export declare function reviewGate(tasks: TaskDefinition[], results: StoredTaskResult[]): GateResult;
export declare function driftFailures(baseline: GitSnapshot | null, current: GitSnapshot | null, reportedFiles: string[]): string[];
//# sourceMappingURL=quality-gates.d.ts.map