import type { LoopSpec } from "./model.js";

export function createDefaultLoopSpec(): LoopSpec {
  const now = new Date().toISOString();
  return {
    schemaVersion: "1.2.0",
    loopId: "auto-default",
    mode: "auto",
    maxSteps: 8,
    stoppingRules: [
      "CLARIFYING",
      "FAILED",
      "PAUSED",
      "VERIFY_FAILED",
      "REVIEW_FAILED",
    ],
    createdAt: now,
    updatedAt: now,
  };
}
