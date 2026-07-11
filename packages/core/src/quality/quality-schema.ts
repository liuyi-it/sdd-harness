import type { StoredTaskResult } from "./quality-gates.js";
import type { TaskDefinition, TddPhase } from "../engines/tdd/tdd-engine.js";
import { SddError } from "../errors.js";
import { isCommandAllowed } from "../security/shell-policy.js";

const phases: TddPhase[] = ["RED", "GREEN", "REFACTOR", "VERIFY"];
const statuses = ["PENDING", "BUILDING", "DONE", "FAILED", "SKIPPED"];

export function parseTasks(raw: string): TaskDefinition[] {
  const value = parseJson(raw, "tasks.json");
  if (!Array.isArray(value)) fail("tasks.json", "必须是数组");
  const ids = new Set<string>();
  const tasks = value.map((entry, index) => {
    const path = `tasks.json[${index}]`;
    record(entry, path);
    const id = text(entry.id, `${path}.id`);
    if (!/^TASK-[A-Z0-9][A-Z0-9-]*$/.test(id))
      fail(`${path}.id`, "任务 ID 格式无效");
    if (ids.has(id)) fail(`${path}.id`, `重复任务 ID ${id}`);
    ids.add(id);
    if (!phases.includes(entry.phase as TddPhase))
      fail(`${path}.phase`, "阶段无效");
    if (!statuses.includes(String(entry.status)))
      fail(`${path}.status`, "状态无效");
    for (const key of [
      "requirements",
      "scenarios",
      "dependsOn",
      "allowedFiles",
      "expectedNewFiles",
      "forbiddenFiles",
      "verification",
      "doneCriteria",
    ]) {
      const values = strings(entry[key], `${path}.${key}`);
      if (new Set(values).size !== values.length)
        fail(`${path}.${key}`, "不得包含重复项");
    }
    (entry.requirements as string[]).forEach((id, item) => {
      if (!/^REQ-\d+$/.test(id))
        fail(`${path}.requirements[${item}]`, "Requirement ID 格式无效");
    });
    (entry.scenarios as string[]).forEach((id, item) => {
      if (!/^REQ-\d+-SC-\d+$/.test(id))
        fail(`${path}.scenarios[${item}]`, "Scenario ID 格式无效");
    });
    if ((entry.requirements as unknown[]).length !== 1)
      fail(`${path}.requirements`, "必须且只能关联一个 Requirement");
    for (const key of ["allowedFiles", "expectedNewFiles", "forbiddenFiles"])
      (entry[key] as string[]).forEach((pattern, item) => {
        if (
          pattern.startsWith("/") ||
          pattern.includes("\\") ||
          pattern.split("/").includes("..") ||
          /[\r\n\0]/.test(pattern)
        )
          fail(`${path}.${key}[${item}]`, "必须是安全相对路径模式");
      });
    (entry.verification as string[]).forEach((command, item) => {
      if (!isCommandAllowed(command))
        fail(`${path}.verification[${item}]`, "命令未在允许清单内");
    });
    text(entry.title, `${path}.title`);
    return entry as unknown as TaskDefinition;
  });
  const taskIds = new Set(tasks.map((task) => task.id));
  tasks.forEach((task, index) => {
    task.dependsOn.forEach((dependency, dependencyIndex) => {
      if (dependency === task.id)
        fail(
          `tasks.json[${index}].dependsOn[${dependencyIndex}]`,
          "任务不得依赖自身",
        );
      if (!taskIds.has(dependency))
        fail(
          `tasks.json[${index}].dependsOn[${dependencyIndex}]`,
          `不存在依赖任务 ${dependency}`,
        );
    });
  });
  const visiting = new Set<string>();
  const visited = new Set<string>();
  const byId = new Map(tasks.map((task) => [task.id, task]));
  const visit = (taskId: string): void => {
    if (visited.has(taskId)) return;
    if (visiting.has(taskId)) fail("tasks.json", `任务依赖图存在环：${taskId}`);
    visiting.add(taskId);
    for (const dependency of byId.get(taskId)?.dependsOn ?? [])
      visit(dependency);
    visiting.delete(taskId);
    visited.add(taskId);
  };
  tasks.forEach((task) => visit(task.id));
  return tasks;
}

export function assertTaskResultIds(
  tasks: TaskDefinition[],
  results: StoredTaskResult[],
): void {
  const taskIds = new Set(tasks.map((task) => task.id));
  results.forEach((result, index) => {
    if (!taskIds.has(result.taskId))
      fail(
        `task-results.json[${index}].taskId`,
        `不存在对应任务 ${result.taskId}`,
      );
  });
}

export function parseTaskResults(raw: string): StoredTaskResult[] {
  const value = parseJson(raw, "task-results.json");
  if (!Array.isArray(value)) fail("task-results.json", "必须是数组");
  const ids = new Set<string>();
  return value.map((entry, index) => {
    const path = `task-results.json[${index}]`;
    record(entry, path);
    const taskId = text(entry.taskId, `${path}.taskId`);
    if (ids.has(taskId)) fail(`${path}.taskId`, `重复结果 ID ${taskId}`);
    ids.add(taskId);
    strings(entry.modifiedFiles, `${path}.modifiedFiles`);
    evidence(entry.tddEvidence, `${path}.tddEvidence`, true);
    evidence(entry.verification, `${path}.verification`, false);
    return entry as unknown as StoredTaskResult;
  });
}

function evidence(value: unknown, path: string, tdd: boolean): void {
  if (!Array.isArray(value)) fail(path, "必须是数组");
  value.forEach((entry, index) => {
    const itemPath = `${path}[${index}]`;
    record(entry, itemPath);
    text(entry.command, `${itemPath}.command`);
    text(entry.output, `${itemPath}.output`);
    if (typeof entry.passed !== "boolean")
      fail(`${itemPath}.passed`, "必须是 boolean");
    if (tdd && !phases.includes(entry.phase as TddPhase))
      fail(`${itemPath}.phase`, "阶段无效");
    if (
      tdd &&
      Object.hasOwn(entry, "expectedFailure") &&
      typeof entry.expectedFailure !== "boolean"
    )
      fail(`${itemPath}.expectedFailure`, "必须是 boolean");
  });
}

function strings(value: unknown, path: string): string[] {
  if (!Array.isArray(value)) fail(path, "必须是数组");
  return value.map((entry, index) => text(entry, `${path}[${index}]`));
}

function text(value: unknown, path: string): string {
  if (typeof value !== "string" || value.trim().length === 0)
    fail(path, "必须是非空字符串");
  if (/[\r\n\0]/.test(value)) fail(path, "包含非法控制字符");
  return value;
}

function record(
  value: unknown,
  path: string,
): asserts value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    fail(path, "必须是对象");
}

function parseJson(raw: string, path: string): unknown {
  try {
    return JSON.parse(raw) as unknown;
  } catch {
    fail(path, "不是有效 JSON");
  }
}

function fail(path: string, message: string): never {
  throw new SddError("E_STATE_CORRUPTED", `${path} ${message}`);
}
