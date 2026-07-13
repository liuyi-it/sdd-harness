import type { CommandResult } from "../contracts.js";
import type { LoopDecision } from "./model.js";

/**
 * DecisionEngine：纯函数，根据 CommandResult 决策下一步动作。
 * 规则见 docs/四期需求文档.md §11.2
 */
export function decide(input: { result: CommandResult }): LoopDecision {
  if (!input.result.ok) {
    if (
      input.result.state === "PLAN_READY" &&
      ["E_VERIFY_FAILED", "E_REVIEW_FAILED"].includes(
        input.result.error?.code ?? "",
      ) &&
      input.result.error?.next === "sdd build next"
    )
      return "CONTINUE";
    if (input.result.error?.code === "E_VERIFY_FAILED")
      return "PAUSE_FOR_HUMAN";
    if (input.result.error?.code === "E_REVIEW_FAILED")
      return "PAUSE_FOR_HUMAN";
    if (input.result.error?.code === "E_SECURITY_BLOCKED") return "FAIL";
    if (input.result.error?.code === "E_STATE_CORRUPTED") return "FAIL";
    return "FAIL";
  }

  const state = input.result.state;

  if (state === "CLARIFYING") return "PAUSE_FOR_CLARIFICATION";
  if (state === "BUILD_WAITING_AGENT") return "PAUSE_FOR_AGENT";
  if (input.result.actionRequired?.type === "AGENT_TASK_EXECUTION")
    return "PAUSE_FOR_AGENT";

  if (state === "BUILD_READY") return "CONTINUE";
  if (state === "VERIFY_READY") return "CONTINUE";
  if (state === "REVIEW_READY") return "CONTINUE";

  if (state === "ARCHIVED") return "DONE";

  return "CONTINUE";
}
