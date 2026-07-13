import type { LoopSpec } from "./model.js";

export function createDefaultLoopSpec(): LoopSpec {
  const now = new Date().toISOString();
  return {
    schemaVersion: "1.3.0",
    loopId: "auto-default",
    mode: "auto",
    maxSteps: 12,
    maxRetriesPerStep: 0,
    maxRepeatedFailures: 2,
    repairPolicy: {
      maxRepairAttemptsPerTask: 2,
      maxRepeatedFailureSignature: 2,
      stopOnScopeExpansion: true,
    },
    stoppingRules: [
      "CLARIFYING",
      "WAITING_AGENT",
      "VERIFY_FAILED",
      "REVIEW_FAILED",
      "SECURITY_BLOCKED",
      "STATE_CORRUPTED",
    ],
    decisionPolicy: "BALANCED",
    createdAt: now,
    updatedAt: now,
  };
}
