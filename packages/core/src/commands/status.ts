import { access } from "node:fs/promises";
import { join } from "node:path";

import { type CommandResult, type Phase } from "../contracts.js";
import { GitInspector } from "../git/git-inspector.js";
import { StateStore, type WorkflowState } from "../state/state-store.js";

/**
 * status 是唯一纯只读的公共命令：
 * - 未初始化时返回 NOT_INITIALIZED
 * - 已初始化时原样回报当前持久化状态和建议下一步命令
 */
export async function runStatus(root: string): Promise<CommandResult> {
  if (!(await exists(join(root, ".sdd", "state.json")))) {
    return {
      ok: true,
      state: "NOT_INITIALIZED",
      exitCode: 0,
      next: "sdd init",
    };
  }
  const state = await new StateStore(root).read();
  const next = nextCommand(state);
  const git = await new GitInspector(root).snapshot();
  const warnings: string[] = [];
  if (state.recoveredFromBackup) {
    warnings.push(
      `状态已从备份或制品恢复，请确认当前阶段后再继续执行${next === undefined ? "" : `：${next}`}`,
    );
  }
  if (state.degraded) {
    warnings.push(
      `当前处于降级模式（degraded mode）：${state.degradedReason ?? "codebase-memory-mcp unavailable"}`,
    );
  }
  if (git.available && git.files.length > 0) {
    warnings.push(
      `检测到执行前已有未提交修改：${git.files.slice(0, 5).join(", ")}${git.files.length > 5 ? ` 等 ${git.files.length} 个文件` : ""}`,
    );
  }
  return {
    ok: true,
    state: state.currentPhase,
    exitCode: 0,
    ...(state.currentChangeId === null
      ? {}
      : { changeId: state.currentChangeId }),
    ...(next === undefined ? {} : { next }),
    ...(warnings.length === 0 ? {} : { warnings }),
    data: state,
  };
}

const NEXT_BY_PHASE: Partial<Record<Phase, string>> = {
  NOT_INITIALIZED: "sdd init",
  INDEX_READY: "sdd new",
  CLARIFYING: "sdd new",
  SPEC_READY: "sdd design",
  DESIGN_READY: "sdd plan",
  PLAN_READY: "sdd build",
  BUILD_READY: "sdd verify",
  VERIFY_READY: "sdd review",
  REVIEW_READY: "sdd archive",
};

function nextCommand(state: WorkflowState): string | undefined {
  if (state.currentPhase === "FAILED" || state.currentPhase === "PAUSED") {
    return state.suggestedCommand ?? undefined;
  }
  return NEXT_BY_PHASE[state.currentPhase];
}

async function exists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}
