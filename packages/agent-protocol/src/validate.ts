import type { AgentTaskResult } from "./types/task-result.js";

const VALID_STATUSES = [
  "SUCCEEDED",
  "FAILED",
  "BLOCKED",
  "SKIPPED",
  "DEGRADED",
];
const VALID_PHASES = ["RED", "GREEN", "REFACTOR"];

function assertString(val: unknown, field: string): asserts val is string {
  if (typeof val !== "string" || val.length === 0) {
    throw new Error(`E_SCHEMA_VALIDATION_FAILED: ${field} 必须是非空字符串`);
  }
}

function assertStringArray(
  val: unknown,
  field: string,
): asserts val is string[] {
  if (!Array.isArray(val)) {
    throw new Error(`E_SCHEMA_VALIDATION_FAILED: ${field} 必须是数组`);
  }
  for (let i = 0; i < val.length; i++) {
    assertString(val[i], `${field}[${i}]`);
  }
}

function assertNumber(val: unknown, field: string): asserts val is number {
  if (typeof val !== "number" || Number.isNaN(val)) {
    throw new Error(`E_SCHEMA_VALIDATION_FAILED: ${field} 必须是数字`);
  }
}

function assertBoolean(val: unknown, field: string): asserts val is boolean {
  if (typeof val !== "boolean") {
    throw new Error(`E_SCHEMA_VALIDATION_FAILED: ${field} 必须是布尔值`);
  }
}

/** 校验 AgentTaskResult 结构合法性 */
export function validateTaskResult(data: unknown): AgentTaskResult {
  if (!data || typeof data !== "object") {
    throw new Error("E_SCHEMA_VALIDATION_FAILED: AgentTaskResult 必须是对象");
  }
  const obj = data as Record<string, unknown>;

  // 顶层必填字段
  if (obj.schemaVersion !== "1.2.0") {
    throw new Error("E_SCHEMA_VALIDATION_FAILED: schemaVersion 必须为 1.2.0");
  }
  assertString(obj.taskId, "taskId");

  if (!VALID_STATUSES.includes(obj.status as string)) {
    throw new Error(
      `E_SCHEMA_VALIDATION_FAILED: status 必须为 ${VALID_STATUSES.join("/")}`,
    );
  }

  assertStringArray(obj.modifiedFiles, "modifiedFiles");
  assertStringArray(obj.createdFiles, "createdFiles");

  // 校验 commandsRun
  if (!Array.isArray(obj.commandsRun)) {
    throw new Error("E_SCHEMA_VALIDATION_FAILED: commandsRun 必须是数组");
  }
  for (let i = 0; i < (obj.commandsRun as unknown[]).length; i++) {
    const cmd = (obj.commandsRun as Record<string, unknown>[])[i]!;
    assertString(cmd.command, `commandsRun[${i}].command`);
    assertStringArray(cmd.args, `commandsRun[${i}].args`);
    assertNumber(cmd.exitCode, `commandsRun[${i}].exitCode`);
    assertBoolean(cmd.passed, `commandsRun[${i}].passed`);
    assertString(cmd.outputSummary, `commandsRun[${i}].outputSummary`);
  }

  // 校验 tddEvidence（可为空数组）
  if (!Array.isArray(obj.tddEvidence)) {
    throw new Error("E_SCHEMA_VALIDATION_FAILED: tddEvidence 必须是数组");
  }
  for (let i = 0; i < (obj.tddEvidence as unknown[]).length; i++) {
    const e = (obj.tddEvidence as Record<string, unknown>[])[i]!;
    if (!VALID_PHASES.includes(e.phase as string)) {
      throw new Error(
        `E_SCHEMA_VALIDATION_FAILED: tddEvidence[${i}].phase 必须为 ${VALID_PHASES.join("/")}`,
      );
    }
    assertString(e.command, `tddEvidence[${i}].command`);
    assertStringArray(e.args, `tddEvidence[${i}].args`);
    assertBoolean(e.passed, `tddEvidence[${i}].passed`);
    assertString(e.outputSummary, `tddEvidence[${i}].outputSummary`);
  }

  // 校验 verification
  if (!Array.isArray(obj.verification)) {
    throw new Error("E_SCHEMA_VALIDATION_FAILED: verification 必须是数组");
  }
  for (let i = 0; i < (obj.verification as unknown[]).length; i++) {
    const v = (obj.verification as Record<string, unknown>[])[i]!;
    assertString(v.command, `verification[${i}].command`);
    assertStringArray(v.args, `verification[${i}].args`);
    assertBoolean(v.passed, `verification[${i}].passed`);
    assertString(v.outputSummary, `verification[${i}].outputSummary`);
  }

  // 校验 notes
  assertStringArray(obj.notes, "notes");
  if (obj.minimality !== undefined) assertMinimalityEvidence(obj.minimality);

  return obj as unknown as AgentTaskResult;
}

function assertMinimalityEvidence(value: unknown): void {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error("E_SCHEMA_VALIDATION_FAILED: minimality 必须是对象");
  const evidence = value as Record<string, unknown>;
  for (const field of [
    "reusedExisting",
    "standardLibraryChoices",
    "nativePlatformChoices",
  ])
    assertStringArray(evidence[field], `minimality.${field}`);
  assertDecisionList(
    evidence.dependenciesAdded,
    "minimality.dependenciesAdded",
    ["name", "manifest", "reason", "requiredBy"],
  );
  assertDecisionList(
    evidence.abstractionsAdded,
    "minimality.abstractionsAdded",
    ["name", "file", "consumers", "reason"],
  );
  assertDecisionList(evidence.deliberateDebts, "minimality.deliberateDebts", [
    "file",
    "ceiling",
    "trigger",
    "upgrade",
  ]);
}

function assertDecisionList(
  value: unknown,
  field: string,
  fields: readonly string[],
): void {
  if (!Array.isArray(value))
    throw new Error(`E_SCHEMA_VALIDATION_FAILED: ${field} 必须是数组`);
  value.forEach((entry, index) => {
    if (!entry || typeof entry !== "object" || Array.isArray(entry))
      throw new Error(
        `E_SCHEMA_VALIDATION_FAILED: ${field}[${index}] 必须是对象`,
      );
    const decision = entry as Record<string, unknown>;
    for (const name of fields) {
      if (name === "requiredBy" || name === "consumers")
        assertStringArray(decision[name], `${field}[${index}].${name}`);
      else assertString(decision[name], `${field}[${index}].${name}`);
    }
  });
}
