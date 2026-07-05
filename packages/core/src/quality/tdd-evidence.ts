import type { TaskExecutionResult } from "../build/task-executor.js";
import type { TaskDefinition, TddPhase } from "../engines/tdd/tdd-engine.js";
import { isCommandAllowed } from "../security/shell-policy.js";

const PHASES: TddPhase[] = ["RED", "GREEN", "REFACTOR", "VERIFY"];

function isRecord(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function nonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

export function taskEvidenceFailures(
  task: TaskDefinition,
  rawResult: TaskExecutionResult,
): string[] {
  const result = rawResult as unknown;
  if (!isRecord(result)) return [`${task.id} 的执行结果格式无效`];
  const evidence = result.tddEvidence;
  if (!Array.isArray(evidence) || evidence.length === 0)
    return [`${task.id} 缺少 ${task.phase} 阶段证据`];
  const failures: string[] = [];
  for (const rawEntry of evidence) {
    if (!isRecord(rawEntry)) {
      failures.push(`${task.id} 的 TDD 证据格式无效`);
      continue;
    }
    const expectedFailurePresent = Object.hasOwn(rawEntry, "expectedFailure");
    if (
      !PHASES.includes(rawEntry.phase as TddPhase) ||
      rawEntry.phase !== task.phase
    )
      failures.push(`${task.id} 包含与任务阶段不匹配的证据`);
    if (!nonEmptyString(rawEntry.command))
      failures.push(`${task.id} 的 TDD 证据命令无效`);
    else if (!isCommandAllowed(rawEntry.command))
      failures.push(`${task.id} 的 TDD 证据命令未在允许清单内`);
    if (!nonEmptyString(rawEntry.output))
      failures.push(`${task.id} 的 TDD 证据缺少输出`);
    if (typeof rawEntry.passed !== "boolean")
      failures.push(`${task.id} 的 TDD 证据 passed 无效`);
    if (task.phase === "RED") {
      if (
        !expectedFailurePresent ||
        rawEntry.expectedFailure !== true ||
        rawEntry.passed !== false
      )
        failures.push(`${task.id} 未证明观察到预期失败`);
    } else if (expectedFailurePresent || rawEntry.passed !== true) {
      failures.push(`${task.id} 的 ${task.phase} 阶段证据未通过`);
    }
  }
  if (task.phase === "VERIFY") {
    const verification = result.verification;
    if (!Array.isArray(verification) || verification.length === 0)
      failures.push(`${task.id} 缺少全部通过的验证证据`);
    else
      for (const rawEntry of verification) {
        if (
          !isRecord(rawEntry) ||
          !nonEmptyString(rawEntry.command) ||
          (nonEmptyString(rawEntry.command) &&
            !isCommandAllowed(rawEntry.command)) ||
          !nonEmptyString(rawEntry.output) ||
          rawEntry.passed !== true
        )
          failures.push(`${task.id} 包含无效或未通过的验证证据`);
      }
  }
  if (!Array.isArray(result.verification))
    failures.push(`${task.id} 的验证证据格式无效`);
  else if (task.phase !== "VERIFY")
    for (const rawEntry of result.verification) {
      if (
        !isRecord(rawEntry) ||
        !nonEmptyString(rawEntry.command) ||
        (nonEmptyString(rawEntry.command) &&
          !isCommandAllowed(rawEntry.command)) ||
        !nonEmptyString(rawEntry.output) ||
        typeof rawEntry.passed !== "boolean"
      )
        failures.push(`${task.id} 的验证证据格式无效`);
    }
  return [...new Set(failures)];
}

export function tddChainFailures(
  tasks: TaskDefinition[],
  results: Array<TaskExecutionResult & { taskId: string }>,
): string[] {
  const failures: string[] = [];
  const requirements = new Set(tasks.flatMap((task) => task.requirements));
  for (const requirement of requirements) {
    const chain = tasks.filter((task) =>
      task.requirements.includes(requirement),
    );
    const phases = chain.map((task) => task.phase);
    const first = chain[0];
    const sameSets =
      first !== undefined &&
      chain.every(
        (task) =>
          setKey(task.requirements) === setKey(first.requirements) &&
          setKey(task.scenarios) === setKey(first.scenarios),
      );
    if (
      chain.length !== PHASES.length ||
      phases.some((phase, index) => phase !== PHASES[index]) ||
      !sameSets
    ) {
      failures.push(
        `${requirement} 的 TDD 阶段链缺失、重复、乱序或场景集合不一致`,
      );
      continue;
    }
    for (const task of chain) {
      const result = results.find((entry) => entry.taskId === task.id);
      if (result !== undefined)
        failures.push(...taskEvidenceFailures(task, result));
    }
    const indexes = chain.map((task) =>
      results.findIndex((result) => result.taskId === task.id),
    );
    if (
      indexes.every((index) => index >= 0) &&
      indexes.some((index, i) => i > 0 && index <= (indexes[i - 1] ?? -1))
    )
      failures.push(`${requirement} 的 TDD 执行证据乱序`);
  }
  return failures;
}

function setKey(values: string[]): string {
  return [...values].sort().join("\0");
}
