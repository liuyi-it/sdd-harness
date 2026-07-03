import { type TaskExecutionResult } from "../build/task-executor.js";
import { type TaskDefinition } from "../engines/tdd/tdd-engine.js";
import { validateTaskFiles } from "../security/task-scope.js";

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
    if (statuses[task.id] !== "DONE") failures.push(`${task.id} is not DONE`);
    const result = results.find((entry) => entry.taskId === task.id);
    if (result === undefined)
      failures.push(`${task.id} has no execution evidence`);
    else if (
      result.verification.length === 0 ||
      result.verification.some((entry) => !entry.passed)
    ) {
      failures.push(`${task.id} verification did not pass`);
    }
  }
  const requirements = [...spec.matchAll(/###\s+(REQ-\d+)/g)]
    .map((match) => match[1])
    .filter((value): value is string => value !== undefined);
  for (const requirement of requirements) {
    if (!tasks.some((task) => task.requirements.includes(requirement))) {
      failures.push(`${requirement} is not linked to a task`);
    }
  }
  if (!spec.includes("Acceptance Criteria:"))
    failures.push("Acceptance Criteria are missing");
  return { passed: failures.length === 0, failures };
}

export function reviewGate(
  tasks: TaskDefinition[],
  results: StoredTaskResult[],
): GateResult {
  const failures: string[] = [];
  for (const result of results) {
    const task = tasks.find((candidate) => candidate.id === result.taskId);
    if (task === undefined) {
      failures.push(`${result.taskId} does not exist in tasks`);
      continue;
    }
    try {
      validateTaskFiles(result.modifiedFiles, task);
    } catch (error) {
      failures.push(error instanceof Error ? error.message : String(error));
    }
    if (result.verification.some((entry) => !entry.passed)) {
      failures.push(`${result.taskId} contains failed verification evidence`);
    }
  }
  return { passed: failures.length === 0, failures };
}
