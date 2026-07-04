import { type TaskExecutionResult } from "../build/task-executor.js";
import { type TaskDefinition } from "../engines/tdd/tdd-engine.js";
import { type GitSnapshot } from "../git/git-inspector.js";
import { validateTaskFiles } from "../security/task-scope.js";

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

export function verifyGate(
  spec: string,
  tasks: TaskDefinition[],
  results: StoredTaskResult[],
  statuses: Record<string, string>,
): GateResult {
  const failures: string[] = [];
  for (const task of tasks) {
    // verify 阶段同时检查“任务状态已完成”和“任务确实有通过的验证证据”。
    if (statuses[task.id] !== "DONE")
      failures.push(`${task.id} 未完成（DONE）`);
    const result = results.find((entry) => entry.taskId === task.id);
    if (result === undefined) failures.push(`${task.id} 缺少执行证据`);
    else if (
      result.verification.length === 0 ||
      result.verification.some((entry) => !entry.passed)
    ) {
      failures.push(`${task.id} 的验证未通过`);
    }
  }
  const requirements = [...spec.matchAll(/###\s+(REQ-\d+)/g)]
    .map((match) => match[1])
    .filter((value): value is string => value !== undefined);
  for (const requirement of requirements) {
    if (!tasks.some((task) => task.requirements.includes(requirement))) {
      failures.push(`${requirement} 未关联到任何任务`);
    }
  }
  if (!spec.includes("Acceptance Criteria:"))
    failures.push("缺少验收标准（Acceptance Criteria）");
  return { passed: failures.length === 0, failures };
}

export function reviewGate(
  tasks: TaskDefinition[],
  results: StoredTaskResult[],
): GateResult {
  const failures: string[] = [];
  for (const result of results) {
    // review 更关注结果与任务声明范围是否一致，而不是重新编排执行顺序。
    const task = tasks.find((candidate) => candidate.id === result.taskId);
    if (task === undefined) {
      failures.push(`${result.taskId} 在任务列表中不存在`);
      continue;
    }
    try {
      validateTaskFiles(result.modifiedFiles, task);
    } catch (error) {
      failures.push(error instanceof Error ? error.message : String(error));
    }
    if (result.verification.some((entry) => !entry.passed)) {
      failures.push(`${result.taskId} 包含未通过的验证证据`);
    }
  }
  return { passed: failures.length === 0, failures };
}

export function driftFailures(
  baseline: GitSnapshot | null,
  current: GitSnapshot | null,
  reportedFiles: string[],
): string[] {
  if (baseline === null || current === null) return [];
  if (!baseline.available || !current.available) return [];
  const reported = new Set(reportedFiles);
  return current.files
    .filter(
      (file) =>
        baseline.hashes[file] === undefined ||
        baseline.hashes[file] !== current.hashes[file],
    )
    .filter((file) => !reported.has(file))
    .map((file) => `未跟踪到任务结果的变更文件：${file}`);
}
