import type { AgentTaskResult } from "./types/task-result.js";

const VALID_STATUSES = ["DONE", "FAILED", "SKIPPED"];
const VALID_PHASES = ["RED", "GREEN", "REFACTOR"];

/** 校验 AgentTaskResult 结构合法性 */
export function validateTaskResult(data: unknown): AgentTaskResult {
  if (!data || typeof data !== "object") {
    throw new Error("E_SCHEMA_VALIDATION_FAILED: AgentTaskResult 必须是对象");
  }
  const obj = data as Record<string, unknown>;

  if (obj.schemaVersion !== "1.0.0") {
    throw new Error("E_SCHEMA_VALIDATION_FAILED: schemaVersion 必须为 1.0.0");
  }

  if (!obj.taskId || typeof obj.taskId !== "string") {
    throw new Error("E_SCHEMA_VALIDATION_FAILED: taskId 必须是非空字符串");
  }

  if (!VALID_STATUSES.includes(obj.status as string)) {
    throw new Error(
      `E_SCHEMA_VALIDATION_FAILED: status 必须为 ${VALID_STATUSES.join("/")}`,
    );
  }

  if (!Array.isArray(obj.modifiedFiles)) {
    throw new Error("E_SCHEMA_VALIDATION_FAILED: modifiedFiles 必须是数组");
  }

  if (!Array.isArray(obj.createdFiles)) {
    throw new Error("E_SCHEMA_VALIDATION_FAILED: createdFiles 必须是数组");
  }

  // 校验 tddEvidence
  if (Array.isArray(obj.tddEvidence)) {
    for (const evidence of obj.tddEvidence) {
      const e = evidence as Record<string, unknown>;
      if (!VALID_PHASES.includes(e.phase as string)) {
        throw new Error(
          `E_SCHEMA_VALIDATION_FAILED: tddEvidence.phase 必须为 ${VALID_PHASES.join("/")}`,
        );
      }
    }
  }

  return obj as unknown as AgentTaskResult;
}
