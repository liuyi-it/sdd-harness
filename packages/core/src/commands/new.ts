import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import type { SpecEngine } from "../engines/spec/spec-engine.js";
import { SddError } from "../errors.js";
import { FileLock } from "../state/file-lock.js";
import { StateStore } from "../state/state-store.js";

/**
 * new 阶段负责接收粗略需求、提出阻塞问题，并在信息充分后生成首批规格制品。
 * 它同时承担“从 CLARIFYING 继续执行”的恢复逻辑。
 */
interface NewArgs {
  requirement?: string;
  changeId?: string;
  answers?: Record<string, string>;
  nonInteractive?: boolean;
}

export async function runNew(
  root: string,
  rawArgs: Record<string, unknown> | undefined,
  engine: SpecEngine,
): Promise<CommandResult> {
  const args = parseArgs(rawArgs);
  const lock = new FileLock(root);
  await lock.acquire("sdd new");
  try {
    const store = new StateStore(root);
    let state = await store.read();
    if (
      state.currentPhase !== "INDEX_READY" &&
      state.currentPhase !== "CLARIFYING" &&
      state.currentPhase !== "ARCHIVED"
    ) {
      throw new SddError(
        state.currentChangeId === null
          ? "E_INVALID_PHASE_COMMAND"
          : "E_ACTIVE_CHANGE_EXISTS",
        `无法在 ${state.currentPhase} 状态下开启新的变更`,
        state.suggestedCommand ?? undefined,
      );
    }
    const continuing = state.currentPhase === "CLARIFYING";
    const parentChangeId =
      state.currentPhase === "ARCHIVED" ? state.currentChangeId : null;
    const startingPhase = state.currentPhase;
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
      ? await readFile(join(runDirectory, "input.md"), "utf8")
      : args.requirement;
    if (requirement === undefined || requirement.trim() === "") {
      throw new SddError("E_MISSING_ARTIFACT", "需求内容不能为空");
    }
    if (!continuing)
      await writeFile(join(runDirectory, "input.md"), requirement, "utf8");
    state = await store.update((current) => ({
      ...current,
      currentChangeId: changeId,
      currentRunId: runId,
      currentPhase: "NEW_STARTED",
      inProgressPhase: "NEW_STARTED",
      previousPhase: startingPhase,
      lastCommand: "sdd new",
      suggestedCommand: "sdd new",
    }));

    const codebaseSummary = await readFile(
      join(root, ".sdd/index/codebase-summary.md"),
      "utf8",
    );
    const analysis = engine.analyze(requirement);
    const unansweredBlockers = analysis.questions.filter(
      (question) =>
        question.severity === "BLOCKER" && !args.answers?.[question.id],
    );
    const generationInput = {
      requirement,
      codebaseSummary,
      ...(args.answers === undefined ? {} : { answers: args.answers }),
    };
    const preview = engine.generate(generationInput);
    if (parentChangeId !== null) {
      preview.proposal = `${preview.proposal}\n\n## Based On Archived Change\n\n- ${parentChangeId}`;
    }
    const writer = new ArtifactWriter();
    for (const [name, content] of Object.entries({
      "proposal.md": preview.proposal,
      "impact.md": preview.impact,
      "questions.md": preview.questions,
    })) {
      await writer.write(join(changeDirectory, name), content, {
        requirement,
        codebaseSummary,
      });
    }
    if (unansweredBlockers.length > 0) {
      if (args.nonInteractive) {
        await store.update((current) => ({
          ...current,
          currentPhase: "FAILED",
          inProgressPhase: null,
          previousPhase: "INDEX_READY",
          failedCommand: "sdd new",
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

    const artifacts = engine.generate(generationInput);
    for (const [name, content] of Object.entries({
      "answers.md": artifacts.answers,
      "assumptions.md": artifacts.assumptions,
      "spec.md": artifacts.spec,
    })) {
      await writer.write(join(changeDirectory, name), content, {
        requirement,
        codebaseSummary,
        answers: args.answers ?? {},
      });
    }
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
    };
  } finally {
    await lock.release();
  }
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
  };
}

function isStringRecord(value: unknown): value is Record<string, string> {
  return (
    typeof value === "object" &&
    value !== null &&
    Object.values(value).every((entry) => typeof entry === "string")
  );
}
