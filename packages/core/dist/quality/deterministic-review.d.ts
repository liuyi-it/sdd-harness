import type { SpecDocument } from "../engines/openspec/model.js";
import type { TaskDefinition } from "../engines/tdd/tdd-engine.js";
import type { GitSnapshot } from "../git/git-inspector.js";
import type { StoredTaskResult } from "./quality-gates.js";
import { type ReviewIssue } from "./review-report.js";
/**
 * 确定性审查不依赖任何语言模型；所有结论由 task 文件列表、Git delta、spec 范围与
 * 命令白名单直接推导。下游 archive 会按 ReviewReport 阻断特定严重度。
 */
export interface DeterministicReviewInput {
    tasks: TaskDefinition[];
    results: readonly StoredTaskResult[];
    baseline: GitSnapshot | null;
    current: GitSnapshot | null;
    spec: SpecDocument;
}
export interface DeterministicReviewOutput {
    issues: ReviewIssue[];
}
/**
 * 复核 review 终止条件：
 * 1. forbiddenFiles 命中；
 * 2. baseline / current Git delta 中存在未被任何任务 reported 的变更；
 * 3. 测试证据中存在未通过的 verification 记录（应被 verify 拦截，但 review 再校验）；
 * 4. 命令证据中出现可疑路径（落到 BLOCKER）。
 */
export declare function runDeterministicReview(input: DeterministicReviewInput): DeterministicReviewOutput;
//# sourceMappingURL=deterministic-review.d.ts.map