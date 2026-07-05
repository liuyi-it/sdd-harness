import type { TaskExecutionResult } from "../build/task-executor.js";
import type { TaskDefinition, TddPhase } from "../engines/tdd/tdd-engine.js";
import { isCommandAllowed } from "../security/shell-policy.js";

const PHASES: TddPhase[] = ["RED", "GREEN", "REFACTOR", "VERIFY"];

export function taskEvidenceFailures(
  task: TaskDefinition,
  result: TaskExecutionResult,
): string[] {
  const evidence = result.tddEvidence;
  if (!Array.isArray(evidence) || evidence.length === 0)
    return [`${task.id} 缺少 ${task.phase} 阶段证据`];
  const failures: string[] = [];
  if (evidence.some((entry) => entry.phase !== task.phase))
    failures.push(`${task.id} 包含与任务阶段不匹配的证据`);
  if (evidence.some((entry) => !isCommandAllowed(entry.command)))
    failures.push(`${task.id} 的 TDD 证据命令未在允许清单内`);
  if (evidence.some((entry) => entry.output.trim().length === 0))
    failures.push(`${task.id} 的 TDD 证据缺少输出`);
  if (task.phase === "RED") {
    if (
      !evidence.some(
        (entry) =>
          entry.phase === "RED" &&
          !entry.passed &&
          entry.expectedFailure === true &&
          entry.output.trim().length > 0,
      )
    )
      failures.push(`${task.id} 未证明观察到预期失败`);
  } else if (
    evidence.some((entry) => !entry.passed || entry.expectedFailure === true)
  ) {
    failures.push(`${task.id} 的 ${task.phase} 阶段证据未通过`);
  }
  if (
    task.phase === "VERIFY" &&
    (result.verification.length === 0 ||
      result.verification.some((entry) => !entry.passed))
  )
    failures.push(`${task.id} 缺少全部通过的验证证据`);
  return failures;
}

export function tddChainFailures(
  tasks: TaskDefinition[],
  results: Array<TaskExecutionResult & { taskId: string }>,
): string[] {
  const failures: string[] = [];
  const groups = new Map<string, TaskDefinition[]>();
  for (const task of tasks) {
    const key = `${[...task.requirements].sort().join(",")}\0${[...task.scenarios].sort().join(",")}`;
    groups.set(key, [...(groups.get(key) ?? []), task]);
  }
  for (const group of groups.values()) {
    const phases = group.map((task) => task.phase);
    if (
      phases.length !== PHASES.length ||
      phases.some((phase, index) => phase !== PHASES[index])
    ) {
      failures.push(
        `${group[0]?.requirements.join(",") ?? "未知需求"} 的 TDD 阶段链缺失、重复或乱序`,
      );
      continue;
    }
    for (const task of group) {
      const result = results.find((entry) => entry.taskId === task.id);
      if (result !== undefined)
        failures.push(...taskEvidenceFailures(task, result));
    }
    const resultIndexes = group.map((task) =>
      results.findIndex((entry) => entry.taskId === task.id),
    );
    if (
      resultIndexes.every((index) => index >= 0) &&
      resultIndexes.some(
        (index, position) =>
          position > 0 && index <= (resultIndexes[position - 1] ?? -1),
      )
    )
      failures.push(
        `${group[0]?.requirements.join(",") ?? "未知需求"} 的 TDD 执行证据乱序`,
      );
  }
  return failures;
}
