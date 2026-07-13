import { readFile } from "node:fs/promises";
import { isAbsolute, join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import {
  ArtifactWriter,
  artifactInputHash,
} from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import { SddError } from "../errors.js";
import {
  GitInspector,
  snapshotFromJson,
  type GitSnapshot,
} from "../git/git-inspector.js";
import { driftFailures, reviewGate } from "../quality/quality-gates.js";
import {
  createReviewIssue,
  createReviewReport,
  type ReviewIssue,
  writeReviewReport,
} from "../quality/review-report.js";
import { runDeterministicReview } from "../quality/deterministic-review.js";
import {
  assertTaskResultIds,
  parseTaskResults,
  parseTasks,
} from "../quality/quality-schema.js";
import { FileLock } from "../state/file-lock.js";
import { readAuthoritativeSpec } from "../quality/traceability.js";
import { scanSecrets } from "../security/secrets-scanner.js";
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
import { prepareRepairTasks } from "./repair-task.js";

/**
 * review 阶段在 verify 通过之后再做一次实现侧审查，
 * 重点确认修改范围、验证证据和任务声明之间没有漂移。
 */
export async function runReview(
  root: string,
  args?: Record<string, unknown>,
  signal?: AbortSignal,
): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd review", undefined, lockOptions(args));
  const store = new StateStore(root);
  let started = false;
  let previousPhase: CommandResult["state"] = "VERIFY_READY";
  let activeChangeId: string | undefined;
  try {
    const state = await store.read();
    assertRecoverableCommandState(state, "sdd review");
    previousPhase = previousStablePhase(state, "VERIFY_READY");
    if (
      state.currentPhase !== "VERIFY_READY" &&
      state.currentPhase !== "REVIEW_READY" &&
      !canResumeCommand(state, "sdd review")
    ) {
      throw new SddError(
        "E_VERIFY_REQUIRED",
        `无法在 ${state.currentPhase} 状态下执行 review`,
        state.suggestedCommand ?? "sdd verify",
      );
    }
    const changeId = requireActiveChangeId(state.currentChangeId, args);
    activeChangeId = changeId;
    await assertChangeWritable(root, changeId);
    const businessRoot = resolveBusinessRoot(root, state);
    const change = join(root, ".sdd", "changes", changeId);
    await store.update((current) => ({
      ...current,
      currentPhase: "REVIEWING",
      inProgressPhase: "REVIEWING",
      previousPhase,
      lastCommand: "sdd review",
      lastError: null,
    }));
    started = true;
    const resultBundle = await withTimeout(
      (async () => {
        const currentSnapshot = await new GitInspector(businessRoot).snapshot();
        const [rawTasks, rawResults] = await Promise.all([
          readFile(join(change, "tasks.json"), "utf8"),
          readFile(join(change, "task-results.json"), "utf8"),
        ]);
        const tasks = parseTasks(rawTasks);
        const results = parseTaskResults(rawResults);
        assertTaskResultIds(tasks, results);
        const rawSpec = await readFile(join(change, "spec.md"), "utf8");
        const authoritative = await readAuthoritativeSpec(change, rawSpec);
        const baseline = snapshotFromJson(
          JSON.parse(await readFile(join(change, "git-baseline.json"), "utf8")),
        );
        const gate = reviewGate(tasks, results);
        const reportedFiles = results.flatMap((result) => result.modifiedFiles);
        const drift = driftFailures(baseline, currentSnapshot, reportedFiles);
        gate.failures.push(...drift);
        gate.passed = gate.failures.length === 0;
        const deterministic = runDeterministicReview({
          tasks,
          results,
          baseline,
          current: currentSnapshot,
          spec: authoritative.document,
        });
        const secretIssues = await detectSecretLeakIssues(
          businessRoot,
          baseline,
          currentSnapshot,
        );
        const reviewReport = createReviewReport({
          changeId,
          fixedPoint: artifactInputHash(currentSnapshot),
          issues: deterministic.issues.concat(secretIssues),
          ...(gate.failures.length === 0
            ? {}
            : {
                issues: deterministic.issues.concat(secretIssues).concat(
                  gate.failures.map((failure) =>
                    failure.includes("未跟踪")
                      ? {
                          id: "RV-" + failure,
                          category: "UNRELATED_CHANGE" as const,
                          severity: "MAJOR" as const,
                          axis: "STANDARDS" as const,
                          message: failure,
                        }
                      : {
                          id: "RV-" + failure,
                          category: "FILE_SCOPE" as const,
                          severity: "MAJOR" as const,
                          axis: "STANDARDS" as const,
                          message: failure,
                        },
                  ),
                ),
              }),
        });
        await writeReviewReport(root, changeId, reviewReport);
        if (reviewReport.result === "BLOCK") {
          const reviewError = new SddError(
            "E_REVIEW_FAILED",
            reviewReport.message,
            "sdd review",
          ) as SddError & { repairFiles?: string[]; findingIds?: string[] };
          reviewError.repairFiles = [
            ...new Set([
              ...changedUnreportedFiles(
                baseline,
                currentSnapshot,
                reportedFiles,
              ),
              ...secretIssues.flatMap((issue) =>
                issue.file === undefined ? [] : [issue.file],
              ),
            ]),
          ];
          reviewError.findingIds = reviewReport.issues
            .filter((issue) => issue.severity === "MAJOR")
            .map((issue) => issue.id);
          throw reviewError;
        }

        if (
          state.currentPhase === "REVIEW_READY" &&
          gate.passed &&
          (await reviewSnapshotUnchanged(change, currentSnapshot))
        ) {
          return {
            reused: true as const,
            currentSnapshot,
          };
        }
        const report = [
          "# 审查报告",
          "",
          "## 概要",
          "",
          gate.passed ? "实现证据与计划一致。" : "审查发现需要修复的问题。",
          "",
          "## 代码质量",
          "",
          "每个已完成任务都具备验证证据。",
          "",
          "## 架构一致性",
          "",
          "改动仍与计划中的任务和需求保持关联。",
          "",
          "## 性能",
          "",
          "未记录到未经审查的性能问题。",
          "",
          "## 简洁性",
          "",
          "未记录到无关的实现证据。",
          "",
          "## 安全",
          "",
          "文件与命令的安全闸门均已生效。",
          "",
          "## 文件范围",
          "",
          gate.passed
            ? "PASS"
            : gate.failures.map((failure) => `- ${failure}`).join("\n"),
          "",
          "## 无关改动",
          "",
          drift.length === 0
            ? "未检测到。"
            : drift.map((item) => `- ${item}`).join("\n"),
          "",
          "## 需修复项",
          "",
          gate.failures.length === 0
            ? "无。"
            : gate.failures.map((failure) => `- ${failure}`).join("\n"),
          "",
          "## 建议",
          "",
          "后续改动请保持在明确的任务范围之内。",
          "",
          "## Result",
          "",
          gate.passed ? "PASS" : "FAIL",
        ].join("\n");
        await new ArtifactWriter().write(
          join(change, "review-report.md"),
          report,
          {
            tasks,
            results,
          },
        );
        await new ArtifactWriter().write(
          join(change, "review-snapshot.json"),
          JSON.stringify(currentSnapshot, null, 2),
          { currentSnapshot },
        );
        return {
          reused: false as const,
          currentSnapshot,
          tasks,
          results,
          gate,
          report,
        };
      })(),
      timeoutMilliseconds(args),
      "sdd review",
      signal,
    );
    if (resultBundle.reused) {
      return {
        ok: true,
        state: "REVIEW_READY",
        exitCode: 0,
        changeId,
        next: "sdd archive",
        data: { alreadyReady: true },
      };
    }
    const { gate } = resultBundle;
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
      failedCommand: null,
      failedReason: null,
      interruptedCommand: null,
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
    const normalized = normalizeCommandError(
      error,
      "E_STATE_CORRUPTED",
      "sdd review",
    );
    if (started) {
      if (
        normalized.code === "E_REVIEW_FAILED" &&
        activeChangeId !== undefined
      ) {
        const repair = await prepareRepairTasks(root, activeChangeId, {
          source: "REVIEW",
          errorCode: "E_REVIEW_FAILED",
          message: normalized.message,
          ...(repairFilesFrom(error) === undefined
            ? {}
            : { requestedFiles: repairFilesFrom(error)! }),
          ...(findingIdsFrom(error) === undefined
            ? {}
            : { findingIds: findingIdsFrom(error)! }),
        });
        throw new SddError(
          normalized.code,
          normalized.message,
          repair.created ? "sdd build next" : "sdd status",
        );
      }
      await persistCommandFailure(store, normalized, {
        command: "sdd review",
        previousPhase,
        inProgressPhase: "REVIEWING",
      });
    }
    throw normalized;
  } finally {
    await lock.release();
  }
}

function changedUnreportedFiles(
  baseline: GitSnapshot | null,
  current: GitSnapshot | null,
  reportedFiles: readonly string[],
): string[] {
  if (
    baseline === null ||
    current === null ||
    !baseline.available ||
    !current.available
  )
    return [];
  const reported = new Set(reportedFiles);
  return current.files.filter(
    (file) =>
      baseline.hashes[file] !== current.hashes[file] && !reported.has(file),
  );
}

function repairFilesFrom(error: unknown): string[] | undefined {
  if (typeof error !== "object" || error === null || !("repairFiles" in error))
    return undefined;
  const files = (error as { repairFiles?: unknown }).repairFiles;
  return Array.isArray(files)
    ? files.filter((file): file is string => typeof file === "string")
    : undefined;
}

function findingIdsFrom(error: unknown): string[] | undefined {
  if (typeof error !== "object" || error === null || !("findingIds" in error))
    return undefined;
  const ids = (error as { findingIds?: unknown }).findingIds;
  return Array.isArray(ids)
    ? ids.filter((id): id is string => typeof id === "string")
    : undefined;
}

async function detectSecretLeakIssues(
  businessRoot: string,
  baseline: GitSnapshot | null,
  current: GitSnapshot | null,
) {
  if (baseline === null || current === null) return [];
  if (!baseline.available || !current.available) return [];
  const issues: ReviewIssue[] = [];
  for (const file of current.files) {
    const baselineHash = baseline.hashes[file];
    const currentHash = current.hashes[file];
    if (baselineHash !== undefined && baselineHash === currentHash) continue;
    const path = join(businessRoot, file);
    let contents: string;
    try {
      contents = await readFile(path, "utf8");
    } catch {
      continue;
    }
    const findings = scanSecrets({ text: contents, file }).findings;
    for (const finding of findings) {
      issues.push(
        createReviewIssue({
          category: "SECRET_LEAK",
          severity: "MAJOR",
          file,
          message: `文件 ${file} 命中敏感信息规则 ${finding.rule}，预览 ${finding.preview}`,
        }),
      );
    }
  }
  return issues;
}

function resolveBusinessRoot(
  controlRoot: string,
  state: Awaited<ReturnType<StateStore["read"]>>,
): string {
  const worktreePath = state.workspace?.worktreePath;
  if (typeof worktreePath !== "string" || worktreePath.length === 0) {
    return controlRoot;
  }
  return isAbsolute(worktreePath)
    ? worktreePath
    : join(controlRoot, worktreePath);
}

function lockOptions(args: Record<string, unknown> | undefined): {
  timeoutMs?: number;
} {
  const timeoutMs = timeoutMilliseconds(args);
  return timeoutMs === undefined ? {} : { timeoutMs };
}

async function reviewSnapshotUnchanged(
  change: string,
  currentSnapshot: ReturnType<GitInspector["snapshot"]> extends Promise<infer T>
    ? T
    : never,
): Promise<boolean> {
  try {
    const writer = new ArtifactWriter();
    if (
      !(await writer.isUnmodified(join(change, "review-report.md"))) ||
      !(await writer.isUnmodified(join(change, "review-snapshot.json")))
    )
      return false;
    const saved = snapshotFromJson(
      JSON.parse(await readFile(join(change, "review-snapshot.json"), "utf8")),
    );
    return JSON.stringify(saved) === JSON.stringify(currentSnapshot);
  } catch {
    return false;
  }
}
