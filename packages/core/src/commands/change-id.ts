import { access } from "node:fs/promises";
import { join } from "node:path";

import { SddError } from "../errors.js";

/**
 * 统一校验显式传入的 changeId 是否与当前活动变更一致。
 * 阶段命令不允许悄悄忽略 `--change`，避免用户误操作到错误的变更目录。
 */
export function requireActiveChangeId(
  activeChangeId: string | null,
  args: Record<string, unknown> | undefined,
): string {
  if (activeChangeId === null) {
    throw new SddError("E_MISSING_CHANGE", "当前没有进行中的变更");
  }
  const requested = args?.changeId;
  if (
    typeof requested === "string" &&
    requested.length > 0 &&
    requested !== activeChangeId
  ) {
    throw new SddError(
      "E_MISSING_CHANGE",
      `指定的变更 ${requested} 与当前活动变更 ${activeChangeId} 不一致`,
    );
  }
  return activeChangeId;
}

export async function assertChangeWritable(
  root: string,
  changeId: string,
): Promise<void> {
  try {
    await access(join(root, ".sdd", "changes", changeId, ".archived"));
    throw new SddError(
      "E_ARCHIVED_READONLY",
      `变更 ${changeId} 已归档，当前为只读状态`,
    );
  } catch (error) {
    if (isMissingFile(error)) return;
    throw error;
  }
}

function isMissingFile(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    error.code === "ENOENT"
  );
}
