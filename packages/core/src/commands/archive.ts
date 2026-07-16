import { createHash } from "node:crypto";
import { readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import { isAbsolute, join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import {
  readCompactPlan,
  readCompactSpec,
} from "../artifacts/change-artifacts.js";
import { type CommandResult } from "../contracts.js";
import { SddError } from "../errors.js";
import { GitInspector, snapshotFromJson } from "../git/git-inspector.js";
import { GitRunner } from "../git-isolation/git-runner.js";
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
import {
  getPolicy,
  resolvePolicyBundle,
  type PolicyRef,
} from "@sdd-harness/agent-policies";

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
    const businessRoot = resolveBusinessRoot(root, state);
    const workspaceMeta = state.workspace ?? null;
    const change = join(root, ".sdd", "changes", changeId);
    if (await hasValidMarker(change, changeId)) {
      await removeExpandedArtifacts(change);
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
    const compactArchive = await withTimeout(
      (async () => {
        const [
          spec,
          design,
          compactSpec,
          plan,
          verifyReport,
          reviewReport,
          verifyReportData,
          reviewReportData,
          results,
          baselineJson,
          verifySnapshotJson,
          reviewSnapshotJson,
        ] = await Promise.all([
          readFile(join(change, "spec.md"), "utf8"),
          readFile(join(change, "design.md"), "utf8"),
          readCompactSpec(change),
          readCompactPlan(change),
          readFile(join(change, "verify-report.md"), "utf8"),
          readFile(join(change, "review-report.md"), "utf8"),
          readFile(join(change, "verify-report.v1.2.json"), "utf8"),
          readFile(join(change, "review-report.v2.json"), "utf8"),
          readFile(join(change, "task-results.json"), "utf8"),
          readFile(join(change, "git-baseline.json"), "utf8"),
          readFile(join(change, "verify-snapshot.json"), "utf8"),
          readFile(join(change, "review-snapshot.json"), "utf8"),
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
        const tasks = parseTasks(JSON.stringify(plan.tasks));
        const parsedResults = parseTaskResults(results);
        assertTaskResultIds(tasks, parsedResults);
        const { document } = await readAuthoritativeSpec(change, spec);
        const [currentSnapshot, baseline, verifySnapshot, reviewSnapshot] =
          await Promise.all([
            new GitInspector(businessRoot).snapshot(),
            parseSnapshot(baselineJson, "git-baseline.json"),
            parseSnapshot(verifySnapshotJson, "verify-snapshot.json"),
            parseSnapshot(reviewSnapshotJson, "review-snapshot.json"),
          ]);
        const finalHead = await resolveFinalHead(
          businessRoot,
          workspaceMeta !== null,
        );
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
        const policyRefs = collectPolicyRefs(tasks, design);
        const minimality = readMinimality(JSON.parse(reviewReportData));
        const policySources = [
          ...new Map(
            policyRefs.flatMap((policy) => {
              const source = getPolicy(policy.id).source;
              return source.project === "ponytail" &&
                source.upstreamCommit !== undefined
                ? [
                    [
                      `${source.project}:${source.upstreamCommit}`,
                      {
                        project: source.project,
                        commit: source.upstreamCommit,
                      },
                    ] as const,
                  ]
                : [];
            }),
          ).values(),
        ];
        const archiveReport = [
          "# 归档报告",
          "",
          "## 变更摘要",
          "",
          spec,
          "",
          "## 已完成任务",
          "",
          plan.tasksMarkdown,
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
          "## 实现简洁性",
          "",
          `- 新增文件：${minimality.metrics.filesAdded}`,
          `- 修改文件：${minimality.metrics.filesModified}`,
          `- 删除文件：${minimality.metrics.filesDeleted}`,
          `- 新增行：${minimality.metrics.linesAdded ?? "未记录"}`,
          `- 删除行：${minimality.metrics.linesDeleted ?? "未记录"}`,
          `- 新增依赖：${minimality.metrics.dependenciesAdded}`,
          `- 删除依赖：${minimality.metrics.dependenciesRemoved}`,
          "",
          "## 有意接受的工程限制",
          "",
          ...(minimality.deliberateDebts.length === 0
            ? ["未记录。"]
            : minimality.deliberateDebts.map(
                (debt) =>
                  `- ${debt.file}:${debt.line}：${debt.ceiling}；触发条件=${debt.trigger}；升级=${debt.upgrade}`,
              )),
          "",
          "## 隔离工作区",
          "",
          `- branchName: ${workspaceMeta?.branchName ?? "(none)"}`,
          `- worktreePath: ${workspaceMeta?.worktreePath ?? "(controlRoot)"}`,
          `- finalHead: ${finalHead}`,
          "",
          "## 风险与回滚",
          "",
          "通过版本控制和有记录的数据迁移来回滚已归档的变更。",
          "",
          "## Policy Traceability",
          "",
          ...policyRefs.map(
            (policy) => `- ${policy.id}@${policy.version} (${policy.digest})`,
          ),
          "",
          "## Policy Upstream Attribution",
          "",
          ...policyRefs.map((policy) => {
            const source = getPolicy(policy.id).source;
            const adapted = source.adaptedFrom?.join(", ") ?? "sdd-native";
            return `- ${policy.id}: ${source.project}; adaptedFrom=${adapted}`;
          }),
          "",
          "## Loop 与修复",
          "",
          `- Loop Run ID: ${state.currentRunId ?? "(manual)"}`,
          `- Repair Tasks: ${tasks.filter((task) => task.sliceType === "REPAIR").length}`,
          "",
          "## 最终结果",
          "",
          "ARCHIVED",
        ].join("\n");
        const archivedAt = new Date().toISOString();
        const archiveMarkdown = `${archiveReport}\n\n---\n\n${traceability}\n`;
        const archiveJson = `${JSON.stringify(
          {
            schemaVersion: "2.0.0",
            changeId,
            archivedAt,
            specification: { markdown: spec, ...compactSpec },
            design,
            plan,
            quality: {
              taskResults: JSON.parse(results),
              verifyReport,
              reviewReport,
              verifyReportData: JSON.parse(verifyReportData),
              reviewReportData: JSON.parse(reviewReportData),
              gitBaseline: JSON.parse(baselineJson),
              verifySnapshot: JSON.parse(verifySnapshotJson),
              reviewSnapshot: JSON.parse(reviewSnapshotJson),
            },
            traceability,
            workspace: {
              branchName: workspaceMeta?.branchName ?? null,
              worktreePath: workspaceMeta?.worktreePath ?? null,
              finalHead,
            },
            policyRefs,
            minimality: {
              metrics: minimality.metrics,
              dependencyDelta: minimality.dependencyDelta,
              deliberateDebts: minimality.deliberateDebts,
              policySources,
            },
          },
          null,
          2,
        )}\n`;
        const stateHash = createHash("sha256")
          .update(JSON.stringify(state))
          .digest("hex");
        const artifactHash = createHash("sha256")
          .update(archiveJson)
          .update(archiveMarkdown)
          .digest("hex");
        const archivedMarker = `${JSON.stringify({ changeId, archivedAt, stateHash: `sha256:${stateHash}`, artifactHash: `sha256:${artifactHash}` }, null, 2)}\n`;
        return {
          archiveJson,
          archiveMarkdown,
          archivedMarker,
        };
      })(),
      timeoutMilliseconds(args),
      "sdd archive",
      signal,
    );
    await writeCompactArchive(change, compactArchive);
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

function readMinimality(value: unknown): {
  metrics: {
    filesAdded: number;
    filesModified: number;
    filesDeleted: number;
    linesAdded: number | null;
    linesDeleted: number | null;
    netLines: number | null;
    dependenciesAdded: number;
    dependenciesRemoved: number;
    deliberateDebtCount: number;
  };
  dependencyDelta: unknown[];
  deliberateDebts: Array<{
    file: string;
    line: number;
    ceiling: string;
    trigger: string;
    upgrade: string;
  }>;
} {
  const fallback = {
    metrics: {
      filesAdded: 0,
      filesModified: 0,
      filesDeleted: 0,
      linesAdded: null,
      linesDeleted: null,
      netLines: null,
      dependenciesAdded: 0,
      dependenciesRemoved: 0,
      deliberateDebtCount: 0,
    },
    dependencyDelta: [],
    deliberateDebts: [],
  };
  if (typeof value !== "object" || value === null) return fallback;
  const minimality = (value as { minimality?: unknown }).minimality;
  if (typeof minimality !== "object" || minimality === null) return fallback;
  const candidate = minimality as Partial<typeof fallback>;
  if (
    typeof candidate.metrics !== "object" ||
    candidate.metrics === null ||
    !Array.isArray(candidate.dependencyDelta) ||
    !Array.isArray(candidate.deliberateDebts)
  )
    return fallback;
  return candidate as typeof fallback;
}

function collectPolicyRefs(
  tasks: ReturnType<typeof parseTasks>,
  design: string,
): PolicyRef[] {
  const bundles = [
    resolvePolicyBundle({ command: "new" }),
    resolvePolicyBundle({
      command: "design",
      ...(design.includes("## design-it-twice")
        ? { actionType: "HIGH_RISK_DESIGN" }
        : {}),
    }),
    resolvePolicyBundle({ command: "plan" }),
    resolvePolicyBundle({ command: "build" }),
    resolvePolicyBundle({ command: "verify" }),
    resolvePolicyBundle({ command: "review" }),
    resolvePolicyBundle({ command: "archive" }),
  ];
  const refs = [
    ...bundles.flatMap((bundle) => bundle.policies),
    ...tasks.flatMap((task) => task.policyRefs ?? []),
  ];
  return [
    ...new Map(
      refs.map((policy) => [
        `${policy.id}@${policy.version}:${policy.digest}`,
        policy,
      ]),
    ).values(),
  ];
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
    const [archiveJson, archiveMarkdown] = await Promise.all([
      readFile(join(change, "archive.json"), "utf8"),
      readFile(join(change, "archive.md"), "utf8"),
    ]);
    const artifactHash = `sha256:${createHash("sha256")
      .update(archiveJson)
      .update(archiveMarkdown)
      .digest("hex")}`;
    if (
      marker.changeId !== changeId ||
      typeof marker.archivedAt !== "string" ||
      Number.isNaN(Date.parse(marker.archivedAt)) ||
      !/^sha256:[a-f0-9]{64}$/.test(String(marker.stateHash)) ||
      marker.artifactHash !== artifactHash
    )
      throw new Error("invalid");
    return true;
  } catch (error) {
    if (error instanceof Error && "code" in error && error.code === "ENOENT")
      return false;
    throw new SddError("E_STATE_CORRUPTED", ".archived 结构无效");
  }
}

async function writeCompactArchive(
  change: string,
  archive: {
    archiveJson: string;
    archiveMarkdown: string;
    archivedMarker: string;
  },
): Promise<void> {
  const nonce = `${process.pid}-${Date.now()}`;
  const staged = [
    { target: join(change, "archive.json"), content: archive.archiveJson },
    { target: join(change, "archive.md"), content: archive.archiveMarkdown },
    { target: join(change, ".archived"), content: archive.archivedMarker },
  ].map((item) => ({ ...item, temporary: `${item.target}.tmp-${nonce}` }));
  try {
    for (const item of staged)
      await writeFile(item.temporary, item.content, "utf8");
    // marker 最后发布；缺少 marker 的中间状态不会被识别为有效归档。
    for (const item of staged) await rename(item.temporary, item.target);
    await removeExpandedArtifacts(change);
  } catch (error) {
    await Promise.all(
      staged.map(({ temporary }) => rm(temporary, { force: true })),
    );
    throw error;
  }
}

async function removeExpandedArtifacts(change: string): Promise<void> {
  const retained = new Set(["archive.json", "archive.md", ".archived"]);
  const entries = await readdir(change, { withFileTypes: true });
  await Promise.all(
    entries
      .filter((entry) => !retained.has(entry.name))
      .map((entry) =>
        rm(join(change, entry.name), {
          recursive: entry.isDirectory(),
          force: true,
        }),
      ),
  );
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

async function resolveFinalHead(
  businessRoot: string,
  requireGitHead: boolean,
): Promise<string> {
  try {
    return await new GitRunner().revParse(businessRoot, "HEAD");
  } catch (error) {
    if (requireGitHead) throw error;
    return "(unavailable)";
  }
}

function parseSnapshot(content: string, name: string) {
  const snapshot = snapshotFromJson(JSON.parse(content));
  if (snapshot === null)
    throw new SddError("E_STATE_CORRUPTED", `${name} 结构无效`);
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
