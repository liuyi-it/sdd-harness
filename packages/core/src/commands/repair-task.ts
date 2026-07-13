import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { resolvePolicyBundle } from "@sdd-harness/agent-policies";

import {
  ArtifactWriter,
  artifactInputHash,
} from "../artifacts/artifact-writer.js";
import type {
  TaskDefinition,
  TddPhase,
} from "../engines/superpowers/protocol.js";
import { parseTasks } from "../quality/quality-schema.js";
import { validateTaskFiles } from "../security/task-scope.js";
import { StateStore } from "../state/state-store.js";

const PHASES: readonly TddPhase[] = ["RED", "GREEN", "REFACTOR", "VERIFY"];

interface RepairLedger {
  schemaVersion: "1.0.0";
  failures: Record<string, { attempts: number; lastSeenAt: string }>;
}

export interface PrepareRepairInput {
  source: "VERIFY" | "REVIEW";
  errorCode: "E_VERIFY_FAILED" | "E_REVIEW_FAILED";
  message: string;
  failingCommand?: string;
  findingIds?: string[];
  requestedFiles?: string[];
}

export async function prepareRepairTasks(
  root: string,
  changeId: string,
  input: PrepareRepairInput,
): Promise<{ created: boolean; paused: boolean; taskIds: string[] }> {
  const change = join(root, ".sdd", "changes", changeId);
  const tasksPath = join(change, "tasks.json");
  const tasks = parseTasks(await readFile(tasksPath, "utf8"));
  const policy = await readRepairPolicy(root);
  const signature = artifactInputHash({
    source: input.source,
    errorCode: input.errorCode,
    message: normalizeFailure(input.message),
    failingCommand: input.failingCommand ?? null,
    findingIds: [...(input.findingIds ?? [])].sort(),
  });
  const ledgerPath = join(change, "repair-attempts.json");
  const ledger = await readLedger(ledgerPath);
  const previousAttempts = ledger.failures[signature]?.attempts ?? 0;
  if (
    previousAttempts >=
    Math.min(
      policy.maxRepairAttemptsPerTask,
      policy.maxRepeatedFailureSignature,
    )
  ) {
    await pauseRepair(root, input, "同一失败签名已达到修复次数限制");
    return { created: false, paused: true, taskIds: [] };
  }

  const allowedFiles = unique(
    tasks.flatMap((task) => [...task.allowedFiles, ...task.expectedNewFiles]),
  );
  const forbiddenFiles = unique(tasks.flatMap((task) => task.forbiddenFiles));
  const verification = unique(tasks.flatMap((task) => task.verification));
  const primaryTask = tasks[0];
  const requirements = primaryTask?.requirements ?? [];
  const scenarios = primaryTask?.scenarios ?? [];
  let requiresScopeExpansion = false;
  if (input.requestedFiles !== undefined) {
    try {
      validateTaskFiles(input.requestedFiles, {
        allowedFiles,
        expectedNewFiles: [],
        forbiddenFiles,
      });
    } catch {
      requiresScopeExpansion = true;
    }
  }
  if (
    policy.stopOnScopeExpansion &&
    (requiresScopeExpansion ||
      allowedFiles.length === 0 ||
      verification.length === 0 ||
      requirements.length !== 1 ||
      scenarios.length === 0)
  ) {
    await pauseRepair(
      root,
      input,
      requiresScopeExpansion
        ? "修复要求扩大 allowedFiles 范围"
        : "修复缺少可复用的文件范围或验证命令",
    );
    return { created: false, paused: true, taskIds: [] };
  }

  const ordinal = nextOrdinal(tasks);
  const runId =
    (await new StateStore(root).read()).currentRunId ?? "manual-run";
  const policyBundle = resolvePolicyBundle({
    command: "build",
    failureCode: input.errorCode,
    actionType: "AGENT_TASK_EXECUTION",
  });
  const repairTasks = PHASES.map((phase, index): TaskDefinition => {
    const id = `TASK-${ordinal}-${phase}`;
    return {
      id,
      title: `${phaseTitle(phase)}：修复 ${input.source} 失败`,
      phase,
      status: "PENDING",
      requirements,
      scenarios,
      dependsOn: index === 0 ? [] : [`TASK-${ordinal}-${PHASES[index - 1]}`],
      allowedFiles,
      expectedNewFiles: [],
      forbiddenFiles,
      verification,
      doneCriteria: [
        `失败签名 ${signature} 已建立反馈并通过 ${phase} 阶段验证`,
      ],
      sliceType: "REPAIR",
      userVisibleOutcome: `恢复 ${input.source.toLowerCase()} 门禁并保持原需求不变`,
      acceptanceCriteria: [input.message],
      policyRefs: policyBundle.policies,
      failureContext: {
        source: input.source,
        errorCode: input.errorCode,
        ...(input.failingCommand === undefined
          ? {}
          : { failingCommand: input.failingCommand }),
        ...(input.findingIds === undefined
          ? {}
          : { findingIds: input.findingIds }),
        previousRunId: runId,
      },
    };
  });
  ledger.failures[signature] = {
    attempts: previousAttempts + 1,
    lastSeenAt: new Date().toISOString(),
  };

  const writer = new ArtifactWriter();
  await writer.write(
    tasksPath,
    `${JSON.stringify(tasks.concat(repairTasks), null, 2)}\n`,
    { source: "repair", signature, attempt: previousAttempts + 1 },
  );
  const tasksMarkdownPath = join(change, "tasks.md");
  const existingMarkdown = await readFile(tasksMarkdownPath, "utf8");
  await writer.write(
    tasksMarkdownPath,
    `${existingMarkdown.trimEnd()}\n\n${renderRepairTasks(repairTasks)}\n`,
    { source: "repair", signature, attempt: previousAttempts + 1 },
  );
  await writer.write(ledgerPath, `${JSON.stringify(ledger, null, 2)}\n`, {
    source: "repair-budget",
    signature,
  });
  const taskIds = repairTasks.map((task) => task.id);
  await new StateStore(root).update((current) => ({
    ...current,
    currentPhase: "PLAN_READY",
    previousPhase: input.source === "VERIFY" ? "BUILD_READY" : "VERIFY_READY",
    inProgressPhase: null,
    failedCommand: `sdd ${input.source.toLowerCase()}`,
    failedReason: input.message,
    interruptedCommand: null,
    recoverable: true,
    lastError: input.errorCode,
    suggestedCommand: "sdd build next",
    tasks: {
      ...current.tasks,
      ...Object.fromEntries(taskIds.map((id) => [id, "PENDING" as const])),
    },
    artifacts: { ...current.artifacts, tasks: "READY", context: "STALE" },
  }));
  return { created: true, paused: false, taskIds };
}

async function readRepairPolicy(root: string): Promise<{
  maxRepairAttemptsPerTask: number;
  maxRepeatedFailureSignature: number;
  stopOnScopeExpansion: boolean;
}> {
  try {
    const parsed = JSON.parse(
      await readFile(join(root, ".sdd", "loop", "loop.json"), "utf8"),
    ) as {
      repairPolicy?: {
        maxRepairAttemptsPerTask?: number;
        maxRepeatedFailureSignature?: number;
        stopOnScopeExpansion?: boolean;
      };
    };
    return {
      maxRepairAttemptsPerTask:
        parsed.repairPolicy?.maxRepairAttemptsPerTask ?? 2,
      maxRepeatedFailureSignature:
        parsed.repairPolicy?.maxRepeatedFailureSignature ?? 2,
      stopOnScopeExpansion: parsed.repairPolicy?.stopOnScopeExpansion ?? true,
    };
  } catch {
    return {
      maxRepairAttemptsPerTask: 2,
      maxRepeatedFailureSignature: 2,
      stopOnScopeExpansion: true,
    };
  }
}

async function readLedger(path: string): Promise<RepairLedger> {
  try {
    const value = JSON.parse(await readFile(path, "utf8")) as RepairLedger;
    return value.schemaVersion === "1.0.0"
      ? value
      : { schemaVersion: "1.0.0", failures: {} };
  } catch {
    return { schemaVersion: "1.0.0", failures: {} };
  }
}

async function pauseRepair(
  root: string,
  input: PrepareRepairInput,
  reason: string,
): Promise<void> {
  await new StateStore(root).update((current) => ({
    ...current,
    currentPhase: "PAUSED",
    previousPhase: input.source === "VERIFY" ? "BUILD_READY" : "VERIFY_READY",
    inProgressPhase: null,
    failedCommand: `sdd ${input.source.toLowerCase()}`,
    failedReason: `${reason}：${input.message}`,
    recoverable: true,
    lastError: input.errorCode,
    suggestedCommand: "sdd status",
  }));
}

function nextOrdinal(tasks: readonly TaskDefinition[]): string {
  const maximum = tasks.reduce((current, task) => {
    const value = Number(task.id.match(/^TASK-(\d{3})-/u)?.[1] ?? 0);
    return Math.max(current, value);
  }, 0);
  return String(maximum + 1).padStart(3, "0");
}

function normalizeFailure(message: string): string {
  return message.replace(/\b\d{4}-\d{2}-\d{2}T[^\s;]+/gu, "<timestamp>").trim();
}

function phaseTitle(phase: TddPhase): string {
  return {
    RED: "复现失败",
    GREEN: "最小修复",
    REFACTOR: "整理实现",
    VERIFY: "重新验证",
  }[phase];
}

function renderRepairTasks(tasks: readonly TaskDefinition[]): string {
  return [
    "## Repair Tasks",
    "",
    ...tasks.flatMap((task) => [
      `### ${task.id} ${task.title}`,
      "",
      `- Slice Type: ${task.sliceType}`,
      `- Depends On: ${task.dependsOn.join(", ") || "无"}`,
      `- Failure: ${task.failureContext?.errorCode ?? "unknown"}`,
      "",
    ]),
  ].join("\n");
}

function unique(values: readonly string[]): string[] {
  return [...new Set(values)].sort();
}
