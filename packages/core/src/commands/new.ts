import { mkdir, readFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import type { SpecEngine } from "../engines/spec/spec-engine.js";
import { SddError } from "../errors.js";
import { FileLock } from "../state/file-lock.js";
import { StateStore } from "../state/state-store.js";
import {
  assertRecoverableCommandState,
  canResumeCommand,
  normalizeCommandError,
  persistCommandFailure,
  previousStablePhase,
} from "./recovery.js";
import { timeoutMilliseconds, withTimeout } from "./timeout.js";

/**
 * new 阶段负责接收粗略需求、提出阻塞问题，并在信息充分后生成首批规格制品。
 * 它同时承担“从 CLARIFYING 继续执行”的恢复逻辑。
 */
interface NewArgs {
  requirement?: string;
  changeId?: string;
  answers?: Record<string, string>;
  nonInteractive?: boolean;
  force?: boolean;
}

export async function runNew(
  root: string,
  rawArgs: Record<string, unknown> | undefined,
  engine: SpecEngine,
  signal?: AbortSignal,
): Promise<CommandResult> {
  const args = parseArgs(rawArgs);
  const lock = new FileLock(root);
  await lock.acquire("sdd new", args.changeId, lockOptions(rawArgs));
  const store = new StateStore(root);
  let started = false;
  let previousPhase: CommandResult["state"] = "INDEX_READY";
  try {
    let state = await store.read();
    const retrying = canResumeCommand(state, "sdd new");
    const repeating =
      state.currentPhase === "SPEC_READY" &&
      (args.changeId === undefined || args.changeId === state.currentChangeId);
    assertRecoverableCommandState(state, "sdd new");
    previousPhase = previousStablePhase(state, "INDEX_READY");
    if (
      state.currentPhase !== "INDEX_READY" &&
      state.currentPhase !== "CLARIFYING" &&
      state.currentPhase !== "ARCHIVED" &&
      !repeating &&
      !retrying
    ) {
      throw new SddError(
        state.currentChangeId === null
          ? "E_INVALID_PHASE_COMMAND"
          : "E_ACTIVE_CHANGE_EXISTS",
        `无法在 ${state.currentPhase} 状态下开启新的变更`,
        state.suggestedCommand ?? undefined,
      );
    }
    const continuing =
      state.currentPhase === "CLARIFYING" || repeating || retrying;
    const parentChangeId =
      state.currentPhase === "ARCHIVED" ? state.currentChangeId : null;
    const changeId = continuing ? state.currentChangeId : args.changeId;
    if (changeId === null || changeId === undefined) {
      throw new SddError("E_MISSING_CHANGE", "缺少必需的变更 id");
    }
    const runId = continuing ? state.currentRunId : `run-${Date.now()}`;
    if (runId === null)
      throw new SddError("E_STATE_CORRUPTED", "处于澄清状态的变更缺少 run id");
    const runDirectory = join(root, ".sdd", "runs", runId);
    const changeDirectory = join(root, ".sdd", "changes", changeId);
    await Promise.all([
      mkdir(runDirectory, { recursive: true }),
      mkdir(changeDirectory, { recursive: true }),
    ]);
    const requirement = continuing
      ? (await readFile(join(runDirectory, "input.md"), "utf8")).replace(
          /\n$/,
          "",
        )
      : args.requirement;
    if (requirement === undefined || requirement.trim() === "") {
      throw new SddError("E_MISSING_ARTIFACT", "需求内容不能为空");
    }
    if (!continuing) {
      await new ArtifactWriter().write(
        join(runDirectory, "input.md"),
        requirement,
        {
          changeId,
          requirement,
        },
      );
    }
    state = await store.update((current) => ({
      ...current,
      currentChangeId: changeId,
      currentRunId: runId,
      currentPhase: "NEW_STARTED",
      inProgressPhase: "NEW_STARTED",
      previousPhase,
      lastCommand: "sdd new",
      lastError: null,
      suggestedCommand: "sdd new",
    }));
    started = true;

    const codebaseSummary = await readFile(
      join(root, ".sdd/index/codebase-summary.md"),
      "utf8",
    );
    const analysis = engine.analyze(requirement, args.answers);
    const unansweredBlockers = analysis.questions.filter(
      (question) =>
        question.severity === "BLOCKER" && !args.answers?.[question.id],
    );
    const generationInput = {
      requirement,
      codebaseSummary,
      ...(args.answers === undefined ? {} : { answers: args.answers }),
    };
    const preview = await withTimeout(
      Promise.resolve(engine.generate(generationInput)),
      timeoutMilliseconds(rawArgs),
      "sdd new",
      signal,
    );
    if (parentChangeId !== null) {
      preview.proposal = `${preview.proposal}\n\n## Based On Archived Change\n\n- ${parentChangeId}`;
    }
    const writer = new ArtifactWriter();
    const requirementInputs = {
      requirement,
      answers: args.answers ?? {},
    };
    await Promise.all([
      writer.write(
        join(changeDirectory, "proposal.md"),
        preview.proposal,
        requirementInputs,
      ),
      writer.write(join(changeDirectory, "impact.md"), preview.impact, {
        requirement,
        codebaseSummary,
      }),
      writer.write(
        join(changeDirectory, "questions.md"),
        preview.questions,
        requirementInputs,
      ),
    ]);
    if (unansweredBlockers.length > 0) {
      if (args.nonInteractive) {
        await store.update((current) => ({
          ...current,
          currentPhase: "FAILED",
          inProgressPhase: "NEW_STARTED",
          previousPhase,
          failedCommand: "sdd new",
          failedReason: "非交互模式下 BLOCKER 问题必须提供答案",
          interruptedCommand: null,
          recoverable: true,
          lastError: "E_UNRESOLVED_BLOCKER",
          suggestedCommand: "sdd new",
        }));
        await new AuditLogger(root).write({
          command: "sdd new",
          phase: "FAILED",
          result: "FAIL",
          changeId,
          message: "E_UNRESOLVED_BLOCKER",
        });
        throw new SddError(
          "E_UNRESOLVED_BLOCKER",
          "非交互模式下 BLOCKER 问题必须提供答案",
          "sdd new",
        );
      }
      const clarifying = await store.update((current) => ({
        ...current,
        currentPhase: "CLARIFYING",
        inProgressPhase: null,
        suggestedCommand: "sdd new",
      }));
      await new AuditLogger(root).write({
        command: "sdd new",
        phase: "CLARIFYING",
        result: "PAUSED",
        changeId,
        message: "等待 BLOCKER 问题的答复",
      });
      return {
        ok: true,
        state: clarifying.currentPhase,
        exitCode: 0,
        changeId,
        next: "sdd new",
      };
    }

    const artifacts = await withTimeout(
      Promise.resolve(engine.generate(generationInput)),
      timeoutMilliseconds(rawArgs),
      "sdd new",
      signal,
    );
    for (const [name, content] of Object.entries({
      "answers.md": artifacts.answers,
      "assumptions.md": artifacts.assumptions,
      "spec.md": artifacts.spec,
    })) {
      await writer.write(join(changeDirectory, name), content, {
        requirement,
        answers: args.answers ?? {},
      });
    }
    const structuredInputs = requirementInputs;
    const structuredOutcomes = await Promise.all([
      writer.writeOrCandidate(
        join(changeDirectory, "spec.delta.md"),
        artifacts.delta,
        structuredInputs,
        { force: args.force === true },
      ),
      writer.writeOrCandidate(
        join(changeDirectory, "spec.model.json"),
        JSON.stringify(artifacts.model, null, 2),
        structuredInputs,
        { force: args.force === true },
      ),
    ]);
    const ready = await store.update((current) => ({
      ...current,
      currentPhase: "SPEC_READY",
      inProgressPhase: null,
      artifacts: {
        ...current.artifacts,
        proposal: "READY",
        impact: "READY",
        spec: "READY",
      },
      failedReason: null,
      interruptedCommand: null,
      recoverable: true,
      suggestedCommand: "sdd design",
      lastError: null,
      failedCommand: null,
    }));
    await new AuditLogger(root).write({
      command: "sdd new",
      phase: ready.currentPhase,
      result: "PASS",
      changeId,
    });
    return {
      ok: true,
      state: ready.currentPhase,
      exitCode: 0,
      changeId,
      next: "sdd design",
      ...(structuredOutcomes.includes("candidate")
        ? {
            warnings: [
              "检测到结构化规格制品的人工修改，已生成 candidate 文件供人工合并",
            ],
          }
        : {}),
    };
  } catch (error) {
    const normalized = normalizeCommandError(
      error,
      "E_STATE_CORRUPTED",
      "sdd new",
    );
    if (started && normalized.code !== "E_UNRESOLVED_BLOCKER") {
      await persistCommandFailure(store, normalized, {
        command: "sdd new",
        previousPhase,
        inProgressPhase: "NEW_STARTED",
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

function parseArgs(args: Record<string, unknown> | undefined): NewArgs {
  if (args === undefined) return {};
  return {
    ...(typeof args.requirement === "string"
      ? { requirement: args.requirement }
      : {}),
    ...(typeof args.changeId === "string" ? { changeId: args.changeId } : {}),
    ...(isStringRecord(args.answers) ? { answers: args.answers } : {}),
    ...(typeof args.nonInteractive === "boolean"
      ? { nonInteractive: args.nonInteractive }
      : {}),
    ...(typeof args.force === "boolean" ? { force: args.force } : {}),
  };
}

function isStringRecord(value: unknown): value is Record<string, string> {
  return (
    typeof value === "object" &&
    value !== null &&
    Object.values(value).every((entry) => typeof entry === "string")
  );
}
