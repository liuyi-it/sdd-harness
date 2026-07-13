import { readFile } from "node:fs/promises";
import { isAbsolute, join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import { SddError } from "../errors.js";
import { GitInspector, snapshotFromJson } from "../git/git-inspector.js";
import { driftFailures, verifyGate } from "../quality/quality-gates.js";
import {
  classifyFailure,
  createVerifyReport,
  type VerifyFailure,
  writeVerifyReport,
} from "../quality/verify-report.js";
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
import { prepareRepairTasks } from "./repair-task.js";
import { readAuthoritativeSpec } from "../quality/traceability.js";
import {
  assertTaskResultIds,
  parseTaskResults,
  parseTasks,
} from "../quality/quality-schema.js";

/**
 * verify 阶段关注“需求是否被任务覆盖，任务是否有通过的执行证据”。
 * 它不检查实现细节优雅与否，只判断是否达到可验证完成状态。
 */
export async function runVerify(
  root: string,
  args?: Record<string, unknown>,
  signal?: AbortSignal,
): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd verify", undefined, lockOptions(args));
  const store = new StateStore(root);
  let started = false;
  let previousPhase: CommandResult["state"] = "BUILD_READY";
  let activeChangeId: string | undefined;
  try {
    const state = await store.read();
    assertRecoverableCommandState(state, "sdd verify");
    previousPhase = previousStablePhase(state, "BUILD_READY");
    if (
      state.currentPhase !== "BUILD_READY" &&
      state.currentPhase !== "VERIFY_READY" &&
      !canResumeCommand(state, "sdd verify")
    ) {
      throw new SddError(
        "E_INVALID_PHASE_COMMAND",
        `无法在 ${state.currentPhase} 状态下执行 verify`,
        state.suggestedCommand ?? undefined,
      );
    }
    const changeId = requireActiveChangeId(state.currentChangeId, args);
    activeChangeId = changeId;
    await assertChangeWritable(root, changeId);
    const businessRoot = resolveBusinessRoot(root, state);
    const change = join(root, ".sdd", "changes", changeId);
    await store.update((current) => ({
      ...current,
      currentPhase: "VERIFYING",
      inProgressPhase: "VERIFYING",
      previousPhase,
      lastCommand: "sdd verify",
      lastError: null,
    }));
    started = true;
    const resultBundle = await withTimeout(
      (async () => {
        const [spec, rawTasks, rawResults, currentState] = await Promise.all([
          readFile(join(change, "spec.md"), "utf8"),
          readFile(join(change, "tasks.json"), "utf8"),
          readFile(join(change, "task-results.json"), "utf8"),
          store.read(),
        ]);
        const tasks = parseTasks(rawTasks);
        const results = parseTaskResults(rawResults);
        assertTaskResultIds(tasks, results);
        const authoritative = await readAuthoritativeSpec(change, spec);
        const gate = verifyGate(
          authoritative.document,
          tasks,
          results,
          currentState.tasks,
        );
        const currentSnapshot = await new GitInspector(businessRoot).snapshot();
        if (
          state.currentPhase === "VERIFY_READY" &&
          gate.passed &&
          (await verifySnapshotUnchanged(change, currentSnapshot))
        ) {
          return {
            reused: true as const,
            currentSnapshot,
          };
        }
        const baseline = snapshotFromJson(
          JSON.parse(await readFile(join(change, "git-baseline.json"), "utf8")),
        );
        const reportedFiles = results.flatMap((result) =>
          Array.isArray(result.modifiedFiles) ? result.modifiedFiles : [],
        );
        const drift = driftFailures(baseline, currentSnapshot, reportedFiles);
        gate.failures.push(...drift);
        gate.passed = gate.failures.length === 0;
        const report = reportDocument(
          "验证报告",
          [
            [
              "任务完成情况",
              gate.failures.filter((failure) => failure.includes("TASK-")),
            ],
            [
              "需求覆盖",
              gate.failures.filter((failure) => failure.includes("REQ-")),
            ],
            [
              "验收标准覆盖",
              gate.failures.filter((failure) => failure.includes("Acceptance")),
            ],
            [
              "测试结果",
              results.flatMap((result) =>
                Array.isArray(result.verification)
                  ? result.verification.map((entry) =>
                      typeof entry === "object" && entry !== null
                        ? `${String(entry.command)}: ${entry.passed === true ? "PASS" : "FAIL"}`
                        : "无效验证证据: FAIL",
                    )
                  : [],
              ),
            ],
            ["边界检查", drift],
            ["漂移检查", drift],
            ["未通过项", gate.failures],
          ],
          gate.passed,
        );
        await new ArtifactWriter().write(
          join(change, "verify-report.md"),
          report,
          {
            spec,
            specModel: authoritative.document,
            tasks,
            results,
          },
        );
        await new ArtifactWriter().write(
          join(change, "verify-snapshot.json"),
          JSON.stringify(currentSnapshot, null, 2),
          { currentSnapshot },
        );
        return {
          reused: false as const,
          currentSnapshot,
          spec,
          tasks,
          results,
          gate,
          report,
          repairFiles: changedUnreportedFiles(
            baseline,
            currentSnapshot,
            reportedFiles,
          ),
        };
      })(),
      timeoutMilliseconds(args),
      "sdd verify",
      signal,
    );
    if (resultBundle.reused) {
      return {
        ok: true,
        state: "VERIFY_READY",
        exitCode: 0,
        changeId,
        next: "sdd review",
        data: { alreadyReady: true },
      };
    }
    const {
      gate,
      tasks,
      results: taskResults,
      spec,
      repairFiles,
    } = resultBundle;
    let requirementCount = 0;
    let scenarioCount = 0;
    try {
      const totalSpec = JSON.parse(spec) as unknown as {
        requirements?: Array<{ scenarios?: unknown[] }>;
      };
      if (Array.isArray(totalSpec?.requirements)) {
        requirementCount = totalSpec.requirements.length;
        scenarioCount = totalSpec.requirements.reduce(
          (acc: number, req: { scenarios?: unknown[] }) =>
            acc + (Array.isArray(req.scenarios) ? req.scenarios.length : 0),
          0,
        );
      }
    } catch {
      // verify-report 仅统计场景数；解析失败时退回到 0，不影响业务失败语义。
    }
    const verifyReport = createVerifyReport({
      changeId,
      counts: {
        requirements: requirementCount,
        scenarios: scenarioCount,
        tasks: tasks.length,
        tests: taskResults.reduce(
          (acc: number, r) =>
            acc + (Array.isArray(r.verification) ? r.verification.length : 0),
          0,
        ),
      },
      failures: gate.failures.map(toVerifyFailure),
    });
    const reportPaths = await writeVerifyReport(root, changeId, verifyReport);
    if (!gate.passed) {
      const error = new SddError(
        "E_VERIFY_FAILED",
        gate.failures.join("; "),
        "sdd verify",
      ) as SddError & {
        reportPaths?: { jsonPath: string; mdPath: string };
        repairFiles?: string[];
      };
      error.reportPaths = reportPaths;
      error.repairFiles = repairFiles;
      throw error;
    }
    const ready = await store.update((current) => ({
      ...current,
      currentPhase: "VERIFY_READY",
      inProgressPhase: null,
      failedCommand: null,
      failedReason: null,
      interruptedCommand: null,
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
    const normalized = normalizeCommandError(
      error,
      "E_STATE_CORRUPTED",
      "sdd verify",
    );
    if (started) {
      if (
        normalized.code === "E_VERIFY_FAILED" &&
        activeChangeId !== undefined
      ) {
        const repair = await prepareRepairTasks(root, activeChangeId, {
          source: "VERIFY",
          errorCode: "E_VERIFY_FAILED",
          message: normalized.message,
          ...(repairFilesFrom(error) === undefined
            ? {}
            : { requestedFiles: repairFilesFrom(error)! }),
        });
        throw new SddError(
          normalized.code,
          normalized.message,
          repair.created ? "sdd build next" : "sdd status",
        );
      }
      await persistCommandFailure(store, normalized, {
        command: "sdd verify",
        previousPhase,
        inProgressPhase: "VERIFYING",
      });
    }
    throw normalized;
  } finally {
    await lock.release();
  }
}

function changedUnreportedFiles(
  baseline: ReturnType<typeof snapshotFromJson>,
  current: Awaited<ReturnType<GitInspector["snapshot"]>>,
  reportedFiles: readonly string[],
): string[] {
  if (baseline === null || !baseline.available || !current.available) return [];
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

async function verifySnapshotUnchanged(
  change: string,
  currentSnapshot: ReturnType<GitInspector["snapshot"]> extends Promise<infer T>
    ? T
    : never,
): Promise<boolean> {
  try {
    const writer = new ArtifactWriter();
    if (
      !(await writer.isUnmodified(join(change, "verify-report.md"))) ||
      !(await writer.isUnmodified(join(change, "verify-snapshot.json")))
    )
      return false;
    const saved = snapshotFromJson(
      JSON.parse(await readFile(join(change, "verify-snapshot.json"), "utf8")),
    );
    return JSON.stringify(saved) === JSON.stringify(currentSnapshot);
  } catch {
    return false;
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
    "## 概要",
    "",
    passed ? "所有质量闸门均已通过。" : "存在一个或多个未通过的质量闸门。",
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

/**
 * 把 verifyGate / drift 的扁平字符串映射到结构化失败对象；解析不出来时回退为 tasks 级别。
 */
function toVerifyFailure(message: string): VerifyFailure {
  if (
    message.includes("TASK-") ||
    message.includes("已处理") ||
    message.includes("未能")
  )
    return classifyFailure("tasks", message, extractEntity(message, "TASK-"));
  if (message.includes("REQ-"))
    return classifyFailure(
      "requirements",
      message,
      extractEntity(message, "REQ-"),
    );
  if (message.includes("SC-"))
    return classifyFailure("scenarios", message, extractEntity(message, "SC-"));
  if (message.includes("Acceptance"))
    return classifyFailure("scenarios", message, "acceptance-criteria");
  if (message.includes("未跟踪到任务结果"))
    return classifyFailure("drift", message);
  if (message.includes("未证明观察到预期失败") || message.includes("缺少"))
    return classifyFailure(
      "tddEvidence",
      message,
      extractEntity(message, "TASK-"),
    );
  return classifyFailure("tasks", message);
}

function extractEntity(message: string, prefix: string): string | undefined {
  const idx = message.indexOf(prefix);
  if (idx < 0) return undefined;
  const tail = message.slice(idx).split(/\s|;/)[0] ?? "";
  return tail || undefined;
}
