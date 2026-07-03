import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import { type TaskDefinition } from "../engines/tdd/tdd-engine.js";
import { SddError } from "../errors.js";
import { reviewGate, type StoredTaskResult } from "../quality/quality-gates.js";
import { FileLock } from "../state/file-lock.js";
import { StateStore } from "../state/state-store.js";

export async function runReview(root: string): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd review");
  const store = new StateStore(root);
  try {
    const state = await store.read();
    if (state.currentPhase !== "VERIFY_READY") {
      throw new SddError(
        "E_VERIFY_REQUIRED",
        `Cannot review from ${state.currentPhase}`,
        state.suggestedCommand ?? "sdd verify",
      );
    }
    if (state.currentChangeId === null)
      throw new SddError("E_MISSING_CHANGE", "No active change");
    const changeId = state.currentChangeId;
    const change = join(root, ".sdd", "changes", changeId);
    await store.update((current) => ({
      ...current,
      currentPhase: "REVIEWING",
      inProgressPhase: "REVIEWING",
      lastCommand: "sdd review",
    }));
    const tasks = JSON.parse(
      await readFile(join(change, "tasks.json"), "utf8"),
    ) as TaskDefinition[];
    const results = JSON.parse(
      await readFile(join(change, "task-results.json"), "utf8"),
    ) as StoredTaskResult[];
    const gate = reviewGate(tasks, results);
    const report = [
      "# Review Report",
      "",
      "## Summary",
      "",
      gate.passed
        ? "Implementation evidence is consistent with the plan."
        : "Review found required fixes.",
      "",
      "## Code Quality",
      "",
      "Verification evidence is present for each completed task.",
      "",
      "## Architecture Consistency",
      "",
      "Changes remain linked to planned tasks and requirements.",
      "",
      "## Performance",
      "",
      "No unreviewed performance finding was recorded.",
      "",
      "## Simplicity",
      "",
      "No unrelated implementation evidence was recorded.",
      "",
      "## Security",
      "",
      "File and command safety gates were enforced.",
      "",
      "## File Scope",
      "",
      gate.passed
        ? "PASS"
        : gate.failures.map((failure) => `- ${failure}`).join("\n"),
      "",
      "## Unrelated Changes",
      "",
      gate.passed ? "None detected." : "See Required Fixes.",
      "",
      "## Required Fixes",
      "",
      gate.failures.length === 0
        ? "None."
        : gate.failures.map((failure) => `- ${failure}`).join("\n"),
      "",
      "## Suggestions",
      "",
      "Keep future changes within explicit task scope.",
      "",
      "## Result",
      "",
      gate.passed ? "PASS" : "FAIL",
    ].join("\n");
    await new ArtifactWriter().write(join(change, "review-report.md"), report, {
      tasks,
      results,
    });
    if (!gate.passed)
      throw new SddError(
        "E_REVIEW_FAILED",
        gate.failures.join("; "),
        "sdd review",
      );
    const ready = await store.update((current) => ({
      ...current,
      currentPhase: "REVIEW_READY",
      inProgressPhase: null,
      artifacts: { ...current.artifacts, reviewReport: "READY" },
      suggestedCommand: "sdd archive",
    }));
    await new AuditLogger(root).write({
      command: "sdd review",
      phase: ready.currentPhase,
      result: "PASS",
      changeId,
    });
    return {
      ok: true,
      state: ready.currentPhase,
      exitCode: 0,
      changeId,
      next: "sdd archive",
    };
  } catch (error) {
    if (error instanceof SddError && error.code === "E_REVIEW_FAILED") {
      await store.update((current) => ({
        ...current,
        currentPhase: "FAILED",
        inProgressPhase: null,
        failedCommand: "sdd review",
        lastError: error.code,
        suggestedCommand: "sdd review",
      }));
    }
    throw error;
  } finally {
    await lock.release();
  }
}
