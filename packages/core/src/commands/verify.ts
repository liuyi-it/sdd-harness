import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import { type TaskDefinition } from "../engines/tdd/tdd-engine.js";
import { SddError } from "../errors.js";
import { type StoredTaskResult, verifyGate } from "../quality/quality-gates.js";
import { FileLock } from "../state/file-lock.js";
import { StateStore } from "../state/state-store.js";

export async function runVerify(root: string): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd verify");
  const store = new StateStore(root);
  try {
    const state = await store.read();
    if (state.currentPhase !== "BUILD_READY") {
      throw new SddError(
        "E_INVALID_PHASE_COMMAND",
        `Cannot verify from ${state.currentPhase}`,
        state.suggestedCommand ?? undefined,
      );
    }
    if (state.currentChangeId === null)
      throw new SddError("E_MISSING_CHANGE", "No active change");
    const changeId = state.currentChangeId;
    const change = join(root, ".sdd", "changes", changeId);
    await store.update((current) => ({
      ...current,
      currentPhase: "VERIFYING",
      inProgressPhase: "VERIFYING",
      lastCommand: "sdd verify",
    }));
    const spec = await readFile(join(change, "spec.md"), "utf8");
    const tasks = JSON.parse(
      await readFile(join(change, "tasks.json"), "utf8"),
    ) as TaskDefinition[];
    const results = JSON.parse(
      await readFile(join(change, "task-results.json"), "utf8"),
    ) as StoredTaskResult[];
    const gate = verifyGate(spec, tasks, results, state.tasks);
    const report = reportDocument(
      "Verify Report",
      [
        [
          "Task Completion",
          gate.failures.filter((failure) => failure.includes("TASK-")),
        ],
        [
          "Requirement Coverage",
          gate.failures.filter((failure) => failure.includes("REQ-")),
        ],
        [
          "Acceptance Criteria Coverage",
          gate.failures.filter((failure) => failure.includes("Acceptance")),
        ],
        [
          "Test Results",
          results.flatMap((result) =>
            result.verification.map(
              (entry) => `${entry.command}: ${entry.passed ? "PASS" : "FAIL"}`,
            ),
          ),
        ],
        ["Boundary Checks", []],
        ["Drift Checks", []],
        ["Failed Items", gate.failures],
      ],
      gate.passed,
    );
    await new ArtifactWriter().write(join(change, "verify-report.md"), report, {
      spec,
      tasks,
      results,
    });
    if (!gate.passed)
      throw new SddError(
        "E_VERIFY_FAILED",
        gate.failures.join("; "),
        "sdd verify",
      );
    const ready = await store.update((current) => ({
      ...current,
      currentPhase: "VERIFY_READY",
      inProgressPhase: null,
      artifacts: { ...current.artifacts, verifyReport: "READY" },
      suggestedCommand: "sdd review",
    }));
    await new AuditLogger(root).write({
      command: "sdd verify",
      phase: ready.currentPhase,
      result: "PASS",
      changeId,
    });
    return {
      ok: true,
      state: ready.currentPhase,
      exitCode: 0,
      changeId,
      next: "sdd review",
    };
  } catch (error) {
    if (error instanceof SddError && error.code === "E_VERIFY_FAILED") {
      await store.update((current) => ({
        ...current,
        currentPhase: "FAILED",
        inProgressPhase: null,
        failedCommand: "sdd verify",
        lastError: error.code,
        suggestedCommand: "sdd verify",
      }));
    }
    throw error;
  } finally {
    await lock.release();
  }
}

function reportDocument(
  title: string,
  sections: Array<[string, string[]]>,
  passed: boolean,
): string {
  return [
    `# ${title}`,
    "",
    "## Summary",
    "",
    passed ? "All gates passed." : "One or more gates failed.",
    "",
    ...sections.flatMap(([heading, items]) => [
      `## ${heading}`,
      "",
      ...(items.length === 0 ? ["PASS"] : items.map((item) => `- ${item}`)),
      "",
    ]),
    "## Result",
    "",
    passed ? "PASS" : "FAIL",
  ].join("\n");
}
