import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import {
  ArtifactWriter,
  artifactInputHash,
} from "../artifacts/artifact-writer.js";
import { renderContextPack } from "../build/context-pack.js";
import { type CommandResult } from "../contracts.js";
import type { TddEngine } from "../engines/tdd/tdd-engine.js";
import type { PlanningInput } from "../engines/superpowers/protocol.js";
import { SddError } from "../errors.js";
import {
  resolveProjectRules,
  type RuleHost,
} from "../project-conventions/rule-resolver.js";
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
 * plan 阶段把设计稿进一步拆成任务、测试计划和上下文包。
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
      impact: await readFile(join(change, "impact.md"), "utf8"),
      codebaseSummary: await readFile(
        join(root, ".sdd/index/codebase-summary.md"),
        "utf8",
      ),
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
      const metaPath = `${join(change, "tasks.md")}.meta.json`;
      const metadata = JSON.parse(await readFile(metaPath, "utf8")) as {
        inputHash: string;
      };
      if (metadata.inputHash === inputHash) {
        try {
          const tasksContent = await readFile(join(change, "tasks.md"), "utf8");
          const testPlanContent = await readFile(
            join(change, "test-plan.md"),
            "utf8",
          );
          const contextContent = await readFile(
            join(change, "context.md"),
            "utf8",
          );
          existingPlan = {
            tasksMarkdown: tasksContent,
            testPlan: testPlanContent,
            context: contextContent,
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
          existingPlan = {
            tasksMarkdown: await readFile(join(change, "tasks.md"), "utf8"),
            testPlan: await readFile(join(change, "test-plan.md"), "utf8"),
            context: await readFile(join(change, "context.md"), "utf8"),
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

    await Promise.all([
      writer.write(join(change, "tasks.md"), artifacts.tasksMarkdown, input),
      writer.write(join(change, "test-plan.md"), artifacts.testPlan, input),
      writer.write(join(change, "context.md"), artifacts.context, input),
    ]);
    await writeFile(
      join(change, "tasks.json"),
      `${JSON.stringify(artifacts.tasks, null, 2)}\n`,
      "utf8",
    );
    const tasksJson = JSON.stringify(artifacts.tasks, null, 2);
    const packDirectory = join(root, ".sdd", "context-packs", changeId);
    await mkdir(packDirectory, { recursive: true });
    const host = readHost(args);
    const projectConventionsHash = await readProjectConventionsHash(root);
    await Promise.all(
      artifacts.tasks.map(async (task) => {
        const content = artifacts.contextPacks[task.id];
        if (content === undefined) {
          throw new SddError(
            "E_STATE_CORRUPTED",
            `缺少任务 ${task.id} 的 Context Pack`,
          );
        }
        const rules = await resolveProjectRules(
          root,
          [...task.allowedFiles, ...task.expectedNewFiles],
          host,
        );
        return writer.write(
          join(packDirectory, `${task.id}.md`),
          renderContextPack({
            body: content,
            rules,
            ...input,
            tasksMarkdown: normalizeArtifactContent(artifacts.tasksMarkdown),
            tasksJson,
            projectConventionsHash,
          }),
          input,
        );
      }),
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

function normalizeArtifactContent(value: string): string {
  return value.endsWith("\n") ? value : `${value}\n`;
}

function readHost(args: Record<string, unknown> | undefined): RuleHost {
  return args?.host === "claude-code" ? "claude-code" : "codex";
}

async function readProjectConventionsHash(root: string): Promise<string> {
  try {
    return artifactInputHash(
      await readFile(join(root, ".sdd", "project", "conventions.json"), "utf8"),
    );
  } catch {
    return artifactInputHash("missing-project-conventions");
  }
}
