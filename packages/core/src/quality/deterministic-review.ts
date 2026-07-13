import type { SpecDocument } from "../engines/openspec/model.js";
import type { TaskDefinition } from "../engines/tdd/tdd-engine.js";
import type { GitSnapshot } from "../git/git-inspector.js";
import type { StoredTaskResult } from "./quality-gates.js";
import {
  createReviewIssue,
  type ReviewIssue,
  type ReviewSeverity,
} from "./review-report.js";

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
export function runDeterministicReview(
  input: DeterministicReviewInput,
): DeterministicReviewOutput {
  const issues: ReviewIssue[] = [];
  const currentFiles = new Set<string>();
  if (input.current !== null && input.current.available) {
    for (const file of input.current.files) currentFiles.add(file);
  }
  for (const task of input.tasks) {
    issues.push(...forbiddenFileIssues(task, currentFiles));
  }
  if (input.baseline !== null && input.current !== null) {
    issues.push(
      ...unreportedChangeIssues(input.baseline, input.current, input.results),
    );
  }
  for (const result of input.results) {
    issues.push(...testingEvidenceIssues(result));
  }
  issues.push(...specCoverageIssues(input.tasks, input.spec));
  return { issues };
}

function specCoverageIssues(
  tasks: readonly TaskDefinition[],
  spec: SpecDocument,
): ReviewIssue[] {
  const requirementIds = new Set(tasks.flatMap((task) => task.requirements));
  const scenarioIds = new Set(tasks.flatMap((task) => task.scenarios));
  const issues: ReviewIssue[] = [];
  for (const requirement of spec.requirements) {
    if (!requirementIds.has(requirement.id)) {
      issues.push(
        createReviewIssue({
          axis: "SPEC",
          category: "BLOCKER",
          severity: "MAJOR",
          message: `需求 ${requirement.id} 没有对应构建任务`,
        }),
      );
    }
    for (const scenario of requirement.scenarios) {
      if (scenarioIds.has(scenario.id)) continue;
      issues.push(
        createReviewIssue({
          axis: "SPEC",
          category: "BLOCKER",
          severity: "MAJOR",
          message: `场景 ${scenario.id} 没有对应构建任务`,
        }),
      );
    }
  }
  return issues;
}

function forbiddenFileIssues(
  task: TaskDefinition,
  currentFiles: ReadonlySet<string>,
): ReviewIssue[] {
  if (!Array.isArray(task.forbiddenFiles) || task.forbiddenFiles.length === 0) {
    return [];
  }
  const issues: ReviewIssue[] = [];
  for (const pattern of task.forbiddenFiles) {
    for (const file of currentFiles) {
      if (!matchesPattern(pattern, file)) continue;
      issues.push(
        createReviewIssue({
          category: "FILE_SCOPE",
          severity: "MAJOR",
          task: task.id,
          file,
          message: `任务 ${task.id} 声明禁止修改 ${pattern}，但 current-run 出现 ${file}`,
        }),
      );
    }
  }
  return issues;
}

function matchesPattern(pattern: string, file: string): boolean {
  if (pattern === file) return true;
  if (pattern.endsWith("/**")) {
    const prefix = pattern.slice(0, -3);
    return file === prefix || file.startsWith(`${prefix}/`);
  }
  if (pattern.startsWith("**/")) {
    return file.endsWith(pattern.slice(3));
  }
  if (pattern.startsWith(".")) {
    return file === pattern.slice(1) || file.endsWith(pattern);
  }
  return false;
}

function unreportedChangeIssues(
  baseline: GitSnapshot,
  current: GitSnapshot,
  results: readonly StoredTaskResult[],
): ReviewIssue[] {
  if (!baseline.available || !current.available) return [];
  const reported = new Set<string>();
  for (const result of results) {
    if (!Array.isArray(result.modifiedFiles)) continue;
    for (const file of result.modifiedFiles) reported.add(file);
  }
  const issues: ReviewIssue[] = [];
  for (const file of current.files) {
    const baselineHash = baseline.hashes[file];
    const currentHash = current.hashes[file];
    const isChanged =
      baselineHash === undefined || baselineHash !== currentHash;
    if (!isChanged) continue;
    if (reported.has(file)) continue;
    issues.push(
      createReviewIssue({
        category: "UNRELATED_CHANGE",
        severity: "MAJOR",
        file,
        message: `文件 ${file} 出现在 current-run diff，但未被任何任务的 modifiedFiles 记录`,
      }),
    );
  }
  return issues;
}

function testingEvidenceIssues(result: StoredTaskResult): ReviewIssue[] {
  if (!Array.isArray(result.verification)) return [];
  const issues: ReviewIssue[] = [];
  for (const entry of result.verification) {
    if (entry === null || typeof entry !== "object") continue;
    if (entry.passed === false) {
      const severity: ReviewSeverity = "MAJOR";
      issues.push(
        createReviewIssue({
          category: "TESTING",
          severity,
          task: result.taskId,
          message: `任务 ${result.taskId} 验证命令 ${String(entry.command)} 未通过`,
        }),
      );
    }
    if (typeof entry.command === "string" && entry.command.includes("rm ")) {
      issues.push(
        createReviewIssue({
          category: "BLOCKER",
          severity: "MAJOR",
          task: result.taskId,
          message: `任务 ${result.taskId} 写入危险命令 ${entry.command}`,
        }),
      );
    }
  }
  return issues;
}
