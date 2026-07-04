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
import { assertChangeWritable, requireActiveChangeId } from "./change-id.js";
import {
  assertRecoverableCommandState,
  canResumeCommand,
  normalizeCommandError,
  persistCommandFailure,
  previousStablePhase,
} from "./recovery.js";
import { timeoutMilliseconds, withTimeout } from "./timeout.js";

/**
 * archive 阶段把整个 change 固化为只读归档：
 * - 生成 traceability 和 archive-report
 * - 记录归档摘要
 * - 将状态推进到 ARCHIVED
 */
export async function runArchive(
  root: string,
  args?: Record<string, unknown>,
  signal?: AbortSignal,
): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd archive", undefined, lockOptions(args));
  const store = new StateStore(root);
  let started = false;
  let previousPhase: CommandResult["state"] = "REVIEW_READY";
  try {
    const state = await store.read();
    assertRecoverableCommandState(state, "sdd archive");
    previousPhase = previousStablePhase(state, "REVIEW_READY");
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
    if (
      state.currentPhase !== "REVIEW_READY" &&
      !canResumeCommand(state, "sdd archive")
    ) {
      throw new SddError(
        "E_REVIEW_REQUIRED",
        `无法在 ${state.currentPhase} 状态下执行 archive`,
        state.suggestedCommand ?? "sdd review",
      );
    }
    const changeId = requireActiveChangeId(state.currentChangeId, args);
    try {
      await assertChangeWritable(root, changeId);
    } catch (error) {
      if (error instanceof SddError && error.code === "E_ARCHIVED_READONLY") {
        return {
          ok: true,
          state: "ARCHIVED",
          exitCode: 0,
          changeId,
        };
      }
      throw error;
    }
    const change = join(root, ".sdd", "changes", changeId);
    await store.update((current) => ({
      ...current,
      currentPhase: "ARCHIVING",
      inProgressPhase: "ARCHIVING",
      previousPhase,
      lastCommand: "sdd archive",
      lastError: null,
    }));
    started = true;
    const { archivedMarker } = await withTimeout(
      (async () => {
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
            "验证报告结果不是 PASS",
            "sdd verify",
          );
        if (!reviewReport.includes("## Result\n\nPASS"))
          throw new SddError(
            "E_REVIEW_REQUIRED",
            "审查报告结果不是 PASS",
            "sdd review",
          );
        const tasks = JSON.parse(taskJson) as TaskDefinition[];
        const parsedResults = JSON.parse(results) as Array<{
          taskId: string;
          modifiedFiles: string[];
          verification: Array<{ command: string }>;
        }>;
        const traceability = [
          "# 需求追溯",
          "",
          ...tasks.flatMap((task) => [
            ...task.requirements.map((requirement) => `## ${requirement}`),
            "",
            "任务：",
            `- ${task.id}`,
            "",
            "文件：",
            ...(
              parsedResults.find((result) => result.taskId === task.id)
                ?.modifiedFiles ?? []
            ).map((file) => `- ${file}`),
            "",
            "测试：",
            ...(
              parsedResults.find((result) => result.taskId === task.id)
                ?.verification ?? []
            ).map((entry) => `- ${entry.command}`),
            "",
          ]),
        ].join("\n");
        const archiveReport = [
          "# 归档报告",
          "",
          "## 变更摘要",
          "",
          spec,
          "",
          "## 已完成任务",
          "",
          tasksText,
          "",
          "## 验证结果",
          "",
          "PASS",
          "",
          "## 审查结果",
          "",
          "PASS",
          "",
          "## 风险与回滚",
          "",
          "通过版本控制和有记录的数据迁移来回滚已归档的变更。",
          "",
          "## 最终结果",
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
        const archivedMarker = `${JSON.stringify({ changeId, archivedAt: new Date().toISOString(), stateHash: `sha256:${stateHash}`, artifactHash: `sha256:${artifactHash}` }, null, 2)}\n`;
        return {
          archivedMarker,
        };
      })(),
      timeoutMilliseconds(args),
      "sdd archive",
      signal,
    );
    await writeFile(join(change, ".archived"), archivedMarker, "utf8");
    const archived = await store.update((current) => ({
      ...current,
      currentPhase: "ARCHIVED",
      inProgressPhase: null,
      failedCommand: null,
      failedReason: null,
      interruptedCommand: null,
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
  } catch (error) {
    const normalized = normalizeCommandError(
      error,
      "E_STATE_CORRUPTED",
      "sdd archive",
    );
    if (started) {
      await persistCommandFailure(store, normalized, {
        command: "sdd archive",
        previousPhase,
        inProgressPhase: "ARCHIVING",
      });
    }
    throw normalized;
  } finally {
    await lock.release();
  }
}

function lockOptions(args: Record<string, unknown> | undefined): {
  timeoutMs?: number;
} {
  const timeoutMs = timeoutMilliseconds(args);
  return timeoutMs === undefined ? {} : { timeoutMs };
}
