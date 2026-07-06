import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import { SddError } from "../errors.js";
import { GitInspector, snapshotFromJson } from "../git/git-inspector.js";
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
import {
  driftFailures,
  reviewGate,
  verifyGate,
} from "../quality/quality-gates.js";
import {
  assertTaskResultIds,
  parseTaskResults,
  parseTasks,
} from "../quality/quality-schema.js";
import {
  readAuthoritativeSpec,
  renderTraceability,
  traceabilityFailures,
} from "../quality/traceability.js";

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
  let markerWritten = false;
  let activeChangeId: string | null = null;
  let previousPhase: CommandResult["state"] = "REVIEW_READY";
  try {
    const state = await store.read();
    assertRecoverableCommandState(state, "sdd archive");
    previousPhase = previousStablePhase(state, "REVIEW_READY");
    if (
      state.currentPhase !== "REVIEW_READY" &&
      state.currentPhase !== "ARCHIVED" &&
      !canResumeCommand(state, "sdd archive")
    ) {
      throw new SddError(
        "E_REVIEW_REQUIRED",
        `无法在 ${state.currentPhase} 状态下执行 archive`,
        state.suggestedCommand ?? "sdd review",
      );
    }
    const changeId = requireActiveChangeId(state.currentChangeId, args);
    activeChangeId = changeId;
    const change = join(root, ".sdd", "changes", changeId);
    if (await hasValidMarker(change, changeId)) {
      const archived = await convergeArchivedState(store);
      await writeArchiveAudit(root, archived.currentPhase, changeId);
      return { ok: true, state: "ARCHIVED", exitCode: 0, changeId };
    }
    await assertChangeWritable(root, changeId);
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
        const writer = new ArtifactWriter();
        await assertPassReport(
          writer,
          join(change, "verify-report.md"),
          verifyReport,
          "E_VERIFY_REQUIRED",
          "sdd verify",
        );
        await assertPassReport(
          writer,
          join(change, "review-report.md"),
          reviewReport,
          "E_REVIEW_REQUIRED",
          "sdd review",
        );
        if (
          !(await writer.isUnmodified(join(change, "verify-snapshot.json"))) ||
          !(await writer.isUnmodified(join(change, "review-snapshot.json")))
        )
          throw new SddError(
            "E_VERIFY_REQUIRED",
            "verify/review 快照 metadata 校验失败",
            "sdd verify",
          );
        const tasks = parseTasks(taskJson);
        const parsedResults = parseTaskResults(results);
        assertTaskResultIds(tasks, parsedResults);
        const { document } = await readAuthoritativeSpec(change, spec);
        const [currentSnapshot, baseline, verifySnapshot, reviewSnapshot] =
          await Promise.all([
            new GitInspector(root).snapshot(),
            readSnapshot(join(change, "git-baseline.json")),
            readSnapshot(join(change, "verify-snapshot.json")),
            readSnapshot(join(change, "review-snapshot.json")),
          ]);
        if (
          !sameSnapshot(currentSnapshot, verifySnapshot) ||
          !sameSnapshot(currentSnapshot, reviewSnapshot)
        )
          throw new SddError(
            "E_VERIFY_REQUIRED",
            "当前 Git 快照与 verify/review 快照不一致",
            "sdd verify",
          );
        const gate = verifyGate(document, tasks, parsedResults, state.tasks);
        const review = reviewGate(tasks, parsedResults);
        const reportedFiles = parsedResults.flatMap(
          (result) => result.modifiedFiles,
        );
        const drift = driftFailures(baseline, currentSnapshot, reportedFiles);
        review.failures.push(...drift);
        review.passed = review.failures.length === 0;
        const artifactFailures = traceabilityFailures(
          document,
          tasks,
          parsedResults,
          true,
        );
        if (!gate.passed)
          throw new SddError(
            "E_VERIFY_REQUIRED",
            gate.failures.join("; "),
            "sdd verify",
          );
        if (!review.passed)
          throw new SddError(
            "E_REVIEW_REQUIRED",
            review.failures.join("; "),
            "sdd review",
          );
        if (artifactFailures.length > 0)
          throw new SddError(
            "E_MISSING_ARTIFACT",
            artifactFailures.join("; "),
            "sdd archive",
          );
        const traceability = renderTraceability(document, tasks, parsedResults);
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
          "## 追踪覆盖与证据摘要",
          "",
          `${document.requirements.length} 个 Requirement、${document.requirements.flatMap((item) => item.scenarios).length} 个 Scenario 均覆盖 RED/GREEN/REFACTOR/VERIFY 证据与最终验证命令。`,
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
        await writer.writeGroupAtomically([
          {
            path: join(change, "traceability.md"),
            content: traceability,
            inputs: { tasks, parsedResults },
          },
          {
            path: join(change, "archive-report.md"),
            content: archiveReport,
            inputs: { verifyReport, reviewReport },
          },
        ]);
        const stateHash = createHash("sha256")
          .update(JSON.stringify(state))
          .digest("hex");
        const artifactHash = createHash("sha256")
          .update(normalizeArtifact(traceability))
          .update(normalizeArtifact(archiveReport))
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
    markerWritten = true;
    const archived = await convergeArchivedState(store);
    await writeArchiveAudit(root, archived.currentPhase, changeId);
    return { ok: true, state: archived.currentPhase, exitCode: 0, changeId };
  } catch (error) {
    const normalized = normalizeCommandError(
      error,
      "E_STATE_CORRUPTED",
      "sdd archive",
    );
    if (markerWritten && activeChangeId !== null) {
      try {
        const archived = await convergeArchivedState(store);
        await writeArchiveAudit(root, archived.currentPhase, activeChangeId);
        return {
          ok: true,
          state: "ARCHIVED",
          exitCode: 0,
          changeId: activeChangeId,
        };
      } catch {
        return {
          ok: false,
          state: "ARCHIVING",
          exitCode: normalized.exitCode,
          changeId: activeChangeId,
          error: normalized.toCommandError(),
        };
      }
    }
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

async function convergeArchivedState(store: StateStore) {
  return store.update((current) => ({
    ...current,
    currentPhase: "ARCHIVED",
    inProgressPhase: null,
    failedCommand: null,
    failedReason: null,
    interruptedCommand: null,
    lastError: null,
    suggestedCommand: null,
    artifacts: {
      ...current.artifacts,
      traceability: "READY",
      archiveReport: "READY",
    },
  }));
}

async function writeArchiveAudit(
  root: string,
  phase: string,
  changeId: string,
): Promise<void> {
  await new AuditLogger(root).write({
    command: "sdd archive",
    phase,
    result: "PASS",
    changeId,
  });
}

async function hasValidMarker(
  change: string,
  changeId: string,
): Promise<boolean> {
  try {
    const value: unknown = JSON.parse(
      await readFile(join(change, ".archived"), "utf8"),
    );
    if (typeof value !== "object" || value === null) throw new Error("invalid");
    const marker = value as Record<string, unknown>;
    const writer = new ArtifactWriter();
    const [traceability, archiveReport] = await Promise.all([
      readFile(join(change, "traceability.md"), "utf8"),
      readFile(join(change, "archive-report.md"), "utf8"),
    ]);
    const artifactHash = `sha256:${createHash("sha256")
      .update(traceability)
      .update(archiveReport)
      .digest("hex")}`;
    if (
      marker.changeId !== changeId ||
      typeof marker.archivedAt !== "string" ||
      Number.isNaN(Date.parse(marker.archivedAt)) ||
      !/^sha256:[a-f0-9]{64}$/.test(String(marker.stateHash)) ||
      marker.artifactHash !== artifactHash ||
      !(await writer.isUnmodified(join(change, "traceability.md"))) ||
      !(await writer.isUnmodified(join(change, "archive-report.md")))
    )
      throw new Error("invalid");
    return true;
  } catch (error) {
    if (error instanceof Error && "code" in error && error.code === "ENOENT")
      return false;
    throw new SddError("E_STATE_CORRUPTED", ".archived 结构无效");
  }
}

function normalizeArtifact(content: string): string {
  return content.endsWith("\n") ? content : `${content}\n`;
}

async function assertPassReport(
  writer: ArtifactWriter,
  path: string,
  report: string,
  code: "E_VERIFY_REQUIRED" | "E_REVIEW_REQUIRED",
  next: string,
): Promise<void> {
  const matches = [...report.matchAll(/^## Result\n\n(PASS|FAIL)$/gm)];
  if (
    !(await writer.isUnmodified(path)) ||
    matches.length !== 1 ||
    matches[0]![1] !== "PASS"
  )
    throw new SddError(
      code,
      `${path.split("/").at(-1)} 不是可信的唯一 PASS 报告`,
      next,
    );
}

async function readSnapshot(path: string) {
  const snapshot = snapshotFromJson(JSON.parse(await readFile(path, "utf8")));
  if (snapshot === null)
    throw new SddError("E_STATE_CORRUPTED", `${path} 结构无效`);
  return snapshot;
}

function sameSnapshot(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function lockOptions(args: Record<string, unknown> | undefined): {
  timeoutMs?: number;
} {
  const timeoutMs = timeoutMilliseconds(args);
  return timeoutMs === undefined ? {} : { timeoutMs };
}
