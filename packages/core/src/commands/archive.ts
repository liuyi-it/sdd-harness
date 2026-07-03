import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import { type TaskDefinition } from "../engines/tdd/tdd-engine.js";
import { SddError } from "../errors.js";
import { FileLock } from "../state/file-lock.js";
import { StateStore } from "../state/state-store.js";

export async function runArchive(root: string): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd archive");
  try {
    const store = new StateStore(root);
    const state = await store.read();
    if (state.currentPhase === "ARCHIVED") {
      return {
        ok: true,
        state: "ARCHIVED",
        exitCode: 0,
        ...(state.currentChangeId === null
          ? {}
          : { changeId: state.currentChangeId }),
      };
    }
    if (state.currentPhase !== "REVIEW_READY") {
      throw new SddError(
        "E_REVIEW_REQUIRED",
        `Cannot archive from ${state.currentPhase}`,
        state.suggestedCommand ?? "sdd review",
      );
    }
    if (state.currentChangeId === null)
      throw new SddError("E_MISSING_CHANGE", "No active change");
    const changeId = state.currentChangeId;
    const change = join(root, ".sdd", "changes", changeId);
    await store.update((current) => ({
      ...current,
      currentPhase: "ARCHIVING",
      inProgressPhase: "ARCHIVING",
      lastCommand: "sdd archive",
    }));
    const [spec, tasksText, verifyReport, reviewReport, taskJson, results] =
      await Promise.all([
        readFile(join(change, "spec.md"), "utf8"),
        readFile(join(change, "tasks.md"), "utf8"),
        readFile(join(change, "verify-report.md"), "utf8"),
        readFile(join(change, "review-report.md"), "utf8"),
        readFile(join(change, "tasks.json"), "utf8"),
        readFile(join(change, "task-results.json"), "utf8"),
      ]);
    if (!verifyReport.includes("## Result\n\nPASS"))
      throw new SddError(
        "E_VERIFY_REQUIRED",
        "Verify report is not PASS",
        "sdd verify",
      );
    if (!reviewReport.includes("## Result\n\nPASS"))
      throw new SddError(
        "E_REVIEW_REQUIRED",
        "Review report is not PASS",
        "sdd review",
      );
    const tasks = JSON.parse(taskJson) as TaskDefinition[];
    const parsedResults = JSON.parse(results) as Array<{
      taskId: string;
      modifiedFiles: string[];
      verification: Array<{ command: string }>;
    }>;
    const traceability = [
      "# Traceability",
      "",
      ...tasks.flatMap((task) => [
        ...task.requirements.map((requirement) => `## ${requirement}`),
        "",
        "Tasks:",
        `- ${task.id}`,
        "",
        "Files:",
        ...(
          parsedResults.find((result) => result.taskId === task.id)
            ?.modifiedFiles ?? []
        ).map((file) => `- ${file}`),
        "",
        "Tests:",
        ...(
          parsedResults.find((result) => result.taskId === task.id)
            ?.verification ?? []
        ).map((entry) => `- ${entry.command}`),
        "",
      ]),
    ].join("\n");
    const archiveReport = [
      "# Archive Report",
      "",
      "## Change Summary",
      "",
      spec,
      "",
      "## Completed Tasks",
      "",
      tasksText,
      "",
      "## Verify Result",
      "",
      "PASS",
      "",
      "## Review Result",
      "",
      "PASS",
      "",
      "## Risk and Rollback",
      "",
      "Use version control and documented data migrations to roll back the archived change.",
      "",
      "## Final Result",
      "",
      "ARCHIVED",
    ].join("\n");
    const writer = new ArtifactWriter();
    await writer.write(join(change, "traceability.md"), traceability, {
      tasks,
      parsedResults,
    });
    await writer.write(join(change, "archive-report.md"), archiveReport, {
      verifyReport,
      reviewReport,
    });
    const stateHash = createHash("sha256")
      .update(JSON.stringify(state))
      .digest("hex");
    const artifactHash = createHash("sha256")
      .update(traceability)
      .update(archiveReport)
      .digest("hex");
    await writeFile(
      join(change, ".archived"),
      `${JSON.stringify({ changeId, archivedAt: new Date().toISOString(), stateHash: `sha256:${stateHash}`, artifactHash: `sha256:${artifactHash}` }, null, 2)}\n`,
      "utf8",
    );
    const archived = await store.update((current) => ({
      ...current,
      currentPhase: "ARCHIVED",
      inProgressPhase: null,
      suggestedCommand: null,
      artifacts: {
        ...current.artifacts,
        traceability: "READY",
        archiveReport: "READY",
      },
    }));
    await new AuditLogger(root).write({
      command: "sdd archive",
      phase: archived.currentPhase,
      result: "PASS",
      changeId,
    });
    return { ok: true, state: archived.currentPhase, exitCode: 0, changeId };
  } finally {
    await lock.release();
  }
}
