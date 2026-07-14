import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import {
  ArtifactWriter,
  artifactInputHash,
} from "../artifacts/artifact-writer.js";
import {
  readCompactPlan,
  readCompactSpec,
} from "../artifacts/change-artifacts.js";
import { resolvePolicyBundle } from "@sdd-harness/agent-policies";
import { type CommandResult } from "../contracts.js";
import type { TddEngine } from "../engines/tdd/tdd-engine.js";
import type { PlanningInput } from "../engines/superpowers/protocol.js";
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
 * plan 阶段把设计稿进一步拆成任务、测试计划和上下文摘要。
 * 这里也是后续 build 阶段“允许改哪些文件”的主要事实来源。
 */
export async function runPlan(
  root: string,
  engine: TddEngine,
  args?: Record<string, unknown>,
  signal?: AbortSignal,
): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd plan", undefined, lockOptions(args));
  const store = new StateStore(root);
  let started = false;
  let previousPhase: CommandResult["state"] = "DESIGN_READY";
  try {
    const state = await store.read();
    const retrying = canResumeCommand(state, "sdd plan");
    assertRecoverableCommandState(state, "sdd plan");
    previousPhase = previousStablePhase(state, "DESIGN_READY");
    if (
      state.currentPhase !== "DESIGN_READY" &&
      state.currentPhase !== "PLAN_READY" &&
      !retrying
    ) {
      throw new SddError(
        "E_INVALID_PHASE_COMMAND",
        `无法在 ${state.currentPhase} 状态下执行 plan`,
        state.suggestedCommand ?? undefined,
      );
    }
    const changeId = requireActiveChangeId(state.currentChangeId, args);
    await assertChangeWritable(root, changeId);
    const change = join(root, ".sdd", "changes", changeId);
    await store.update((current) => ({
      ...current,
      currentPhase: "PLANNING",
      inProgressPhase: "PLANNING",
      previousPhase,
      lastCommand: "sdd plan",
      lastError: null,
    }));
    started = true;
    const input: PlanningInput = {
      spec: await readFile(join(change, "spec.md"), "utf8"),
      design: await readFile(join(change, "design.md"), "utf8"),
      impact: (await readCompactSpec(change)).impact,
      codebaseSummary: await readFile(
        join(root, ".sdd/index/codebase-summary.md"),
        "utf8",
      ),
      policyBundle: resolvePolicyBundle({
        command: "plan",
        phase: "DESIGN_READY",
      }),
    };
    const artifacts = await withTimeout(
      Promise.resolve(engine.generatePlan(input)),
      timeoutMilliseconds(args),
      "sdd plan",
      signal,
    );
    const writer = new ArtifactWriter();
    const force = args?.force === true;
    const inputHash = artifactInputHash(input);
    let existingPlan: PlanningInput["existingPlan"];
    let unchanged = false;

    try {
      const metadata = await writer.metadata(join(change, "plan.json"));
      if (metadata === undefined) throw new Error("缺少 plan 制品摘要");
      if (metadata.inputHash === inputHash) {
        try {
          const plan = await readCompactPlan(change);
          existingPlan = {
            tasksMarkdown: plan.tasksMarkdown,
            testPlan: plan.testPlan,
            context: plan.context,
          };
          unchanged = true;
        } catch {
          // 文件不完整，不算 unchanged
        }
      }
    } catch {
      // 文件不存在
    }

    if (unchanged) {
      await store.update((current) => ({
        ...current,
        currentPhase: "PLAN_READY",
        inProgressPhase: null,
        suggestedCommand: "sdd build next",
        artifacts: {
          ...current.artifacts,
          tasks: "READY" as const,
          testPlan: "READY" as const,
          context: "READY" as const,
        },
      }));
      return {
        ok: true,
        state: "PLAN_READY",
        exitCode: 0,
        changeId,
        next: "sdd build next",
        data: { alreadyReady: true },
      };
    }

    if (!force) {
      if (existingPlan === undefined) {
        try {
          const plan = await readCompactPlan(change);
          existingPlan = {
            tasksMarkdown: plan.tasksMarkdown,
            testPlan: plan.testPlan,
            context: plan.context,
          };
        } catch {
          // 文件不存在，不用合并
        }
      }
      if (existingPlan !== undefined) {
        input.existingPlan = existingPlan;
        const merged = await engine.generatePlan(input);
        artifacts.tasksMarkdown = merged.tasksMarkdown;
        artifacts.testPlan = merged.testPlan;
        artifacts.context = merged.context;
        artifacts.tasks = merged.tasks;
        artifacts.contextPacks = merged.contextPacks;
      }
    }

    await writer.write(
      join(change, "plan.json"),
      JSON.stringify(
        {
          schemaVersion: "2.0.0",
          tasks: artifacts.tasks,
          tasksMarkdown: artifacts.tasksMarkdown,
          testPlan: artifacts.testPlan,
          context: artifacts.context,
        },
        null,
        2,
      ),
      input,
    );
    const tasks = Object.fromEntries(
      artifacts.tasks.map((task) => [task.id, task.status]),
    );
    const ready = await store.update((current) => ({
      ...current,
      currentPhase: "PLAN_READY",
      inProgressPhase: null,
      failedCommand: null,
      failedReason: null,
      interruptedCommand: null,
      tasks,
      artifacts: {
        ...current.artifacts,
        tasks: "READY",
        testPlan: "READY",
        context: "READY",
      },
      suggestedCommand: "sdd build next",
    }));
    await new AuditLogger(root).write({
      command: "sdd plan",
      phase: ready.currentPhase,
      result: "PASS",
      changeId,
    });
    return {
      ok: true,
      state: ready.currentPhase,
      exitCode: 0,
      changeId,
      next: "sdd build next",
    };
  } catch (error) {
    const normalized = normalizeCommandError(
      error,
      "E_STATE_CORRUPTED",
      "sdd plan",
    );
    if (started) {
      await persistCommandFailure(store, normalized, {
        command: "sdd plan",
        previousPhase,
        inProgressPhase: "PLANNING",
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
