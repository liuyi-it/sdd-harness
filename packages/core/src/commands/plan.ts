import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import {
  ArtifactWriter,
  artifactInputHash,
} from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import type { TddEngine } from "../engines/tdd/tdd-engine.js";
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
    const input = {
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
    const outcomes = await Promise.all([
      writer.writeOrCandidate(
        join(change, "tasks.md"),
        artifacts.tasksMarkdown,
        input,
        { force: args?.force === true },
      ),
      writer.writeOrCandidate(
        join(change, "test-plan.md"),
        artifacts.testPlan,
        input,
        { force: args?.force === true },
      ),
      writer.writeOrCandidate(
        join(change, "context.md"),
        artifacts.context,
        input,
        { force: args?.force === true },
      ),
    ]);
    if (outcomes.every((outcome) => outcome === "unchanged")) {
      return {
        ok: true,
        state: "PLAN_READY",
        exitCode: 0,
        changeId,
        next: "sdd build",
        data: { alreadyReady: true },
      };
    }
    if (outcomes.some((outcome) => outcome === "candidate")) {
      return {
        ok: true,
        state: "PLAN_READY",
        exitCode: 0,
        changeId,
        next: "sdd build",
        warnings: ["plan 输入已变化；已生成候选制品供人工合并"],
      };
    }
    await writeFile(
      join(change, "tasks.json"),
      `${JSON.stringify(artifacts.tasks, null, 2)}\n`,
      "utf8",
    );
    const tasksJson = JSON.stringify(artifacts.tasks, null, 2);
    const packDirectory = join(root, ".sdd", "context-packs", changeId);
    await mkdir(packDirectory, { recursive: true });
    await Promise.all(
      Object.entries(artifacts.contextPacks).map(([taskId, content]) =>
        writer.write(
          join(packDirectory, `${taskId}.md`),
          contextPackWithMetadata(content, {
            ...input,
            tasksMarkdown: normalizeArtifactContent(artifacts.tasksMarkdown),
            tasksJson,
          }),
          input,
        ),
      ),
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
      suggestedCommand: "sdd build",
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
      next: "sdd build",
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

function contextPackWithMetadata(
  content: string,
  input: {
    spec: string;
    design: string;
    impact: string;
    tasksMarkdown: string;
    tasksJson: string;
    codebaseSummary: string;
  },
): string {
  const metadata = [
    "<!-- Context Pack Metadata",
    `Codebase Index Hash: ${artifactInputHash(input.codebaseSummary)}`,
    `Source Artifact Hash: ${artifactInputHash({
      spec: input.spec,
      design: input.design,
      impact: input.impact,
      tasksMarkdown: input.tasksMarkdown,
      tasksJson: input.tasksJson,
    })}`,
    `Generated At: ${new Date().toISOString()}`,
    "-->",
    "",
  ].join("\n");
  return truncateUtf8(`${metadata}${content}`, 30 * 1024);
}

function truncateUtf8(value: string, maxBytes: number): string {
  if (Buffer.byteLength(value) <= maxBytes) return value;
  let end = value.length;
  while (end > 0 && Buffer.byteLength(value.slice(0, end)) > maxBytes - 32)
    end -= 1;
  return `${value.slice(0, end)}\n\n[Context truncated]\n`;
}

function normalizeArtifactContent(value: string): string {
  return value.endsWith("\n") ? value : `${value}\n`;
}
