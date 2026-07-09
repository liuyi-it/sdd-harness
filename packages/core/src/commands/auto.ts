import { createDefaultLoopSpec } from "../loop/loop-spec.js";
import { LoopStore } from "../loop/loop-store.js";
import { SddError } from "../errors.js";
import { StateStore } from "../state/state-store.js";

interface AutoArgs {
  resume?: string;
  restart?: boolean;
}

export interface PreparedAutoLoop {
  loopId: string;
  runId: string;
  maxSteps: number;
}

export async function prepareAutoLoop(
  root: string,
  args: Record<string, unknown> | undefined,
): Promise<PreparedAutoLoop> {
  const options = parseArgs(args);
  if (options.resume !== undefined && options.restart === true) {
    throw new SddError(
      "E_INVALID_PHASE_COMMAND",
      "resume 与 restart 不能同时使用",
      "sdd auto",
    );
  }

  const store = new StateStore(root);
  const loops = new LoopStore(root);
  const state = await store.read();
  const spec = await readLoopSpec(loops);

  if (options.resume !== undefined) {
    const run = await loops.readRun(options.resume);
    await store.update((current) => ({
      ...current,
      currentRunId: run.runId,
      activeLoop: {
        loopId: run.loopId,
        runId: run.runId,
        status: "RUNNING",
      },
    }));
    return { loopId: run.loopId, runId: run.runId, maxSteps: spec.maxSteps };
  }

  if (options.restart === true && state.activeLoop !== null) {
    const currentLoop = state.activeLoop as {
      loopId: string;
      runId: string;
      status: string;
    };
    const existing = await loops.readRun(currentLoop.runId);
    await loops.writeRun({
      ...existing,
      status: "ABORTED",
      endedAt: new Date().toISOString(),
    });
    const runId = `run-${Date.now()}`;
    await loops.writeRun({
      schemaVersion: "1.3.0" as const,
      runId,
      loopId: currentLoop.loopId,
      status: "RUNNING",
      startedAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      currentStep: 0,
      steps: [],
    });
    await store.update((current) => ({
      ...current,
      currentRunId: runId,
      activeLoop: {
        loopId: currentLoop.loopId,
        runId,
        status: "RUNNING",
      },
    }));
    return { loopId: currentLoop.loopId, runId, maxSteps: spec.maxSteps };
  }

  const currentLoop =
    state.activeLoop !== null &&
    typeof state.activeLoop === "object" &&
    "loopId" in state.activeLoop &&
    "runId" in state.activeLoop
      ? (state.activeLoop as { loopId: string; runId: string; status: string })
      : null;
  const runId = currentLoop?.runId ?? state.currentRunId ?? `run-${Date.now()}`;
  const loopId = currentLoop?.loopId ?? spec.loopId;
  if (!(await loops.hasRun(runId))) {
    await loops.writeRun({
      schemaVersion: "1.3.0" as const,
      runId,
      loopId,
      status: "RUNNING",
      startedAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      currentStep: 0,
      steps: [],
    });
  }
  if (currentLoop === null) {
    await store.update((current) => ({
      ...current,
      currentRunId: runId,
      activeLoop: {
        loopId,
        runId,
        status: "RUNNING",
      },
    }));
  }
  return { loopId, runId, maxSteps: spec.maxSteps };
}

export async function recordAutoStep(
  root: string,
  runId: string,
  step: {
    command: string;
    status: "SUCCEEDED" | "FAILED" | "PAUSED" | "BLOCKED" | "ARCHIVED";
    startedAt: string;
    endedAt: string;
  },
): Promise<void> {
  const loops = new LoopStore(root);
  const run = await loops.readRun(runId);
  await loops.writeRun({
    ...run,
    steps: [
      ...run.steps,
      {
        step: run.steps.length + 1,
        kind: "COMMAND" as const,
        command: step.command,
        phaseBefore: run.steps.length > 0
          ? (run.steps[run.steps.length - 1]!.phaseAfter ?? run.steps[run.steps.length - 1]!.phaseBefore)
          : "NOT_INITIALIZED",
        status:
          step.status === "BLOCKED"
            ? "BLOCKED"
            : step.status === "ARCHIVED"
              ? "SUCCEEDED"
              : step.status,
        startedAt: step.startedAt,
        endedAt: step.endedAt,
      },
    ],
    status:
      step.status === "ARCHIVED"
        ? "ARCHIVED"
        : step.status === "BLOCKED"
          ? "FAILED"
          : step.status,
    ...(step.status === "ARCHIVED" ||
    step.status === "FAILED" ||
    step.status === "PAUSED"
      ? { endedAt: step.endedAt }
      : {}),
  });
}

export async function finalizeAutoLoop(
  root: string,
  runId: string,
  status: "RUNNING" | "PAUSED" | "FAILED" | "ARCHIVED",
): Promise<void> {
  const loops = new LoopStore(root);
  const store = new StateStore(root);
  const run = await loops.readRun(runId);
  await loops.writeRun({
    ...run,
    status,
    ...(status === "ARCHIVED" || status === "FAILED" || status === "PAUSED"
      ? { endedAt: new Date().toISOString() }
      : {}),
  });
  await store.update((current) => ({
    ...current,
    activeLoop:
      current.activeLoop === null ||
      typeof current.activeLoop !== "object" ||
      !("loopId" in current.activeLoop)
        ? current.activeLoop
        : {
            ...(current.activeLoop as Record<string, unknown>),
            runId,
            status: status === "ARCHIVED" ? "SUCCEEDED" : status,
          },
  }));
}

function parseArgs(args: Record<string, unknown> | undefined): AutoArgs {
  return {
    ...(typeof args?.resume === "string" ? { resume: args.resume } : {}),
    ...(args?.restart === true ? { restart: true } : {}),
  };
}

async function readLoopSpec(loops: LoopStore) {
  try {
    return await loops.readSpec();
  } catch {
    return createDefaultLoopSpec();
  }
}
