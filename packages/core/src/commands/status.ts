import { access } from "node:fs/promises";
import { join } from "node:path";

import { type CommandResult, type Phase } from "../contracts.js";
import { StateStore, type WorkflowState } from "../state/state-store.js";

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
  return {
    ok: true,
    state: state.currentPhase,
    exitCode: 0,
    ...(state.currentChangeId === null
      ? {}
      : { changeId: state.currentChangeId }),
    ...(next === undefined ? {} : { next }),
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
