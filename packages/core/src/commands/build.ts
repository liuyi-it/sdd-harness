import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import {
  type TaskExecutionResult,
  type TaskExecutor,
} from "../build/task-executor.js";
import { type CommandResult } from "../contracts.js";
import { type TaskDefinition } from "../engines/tdd/tdd-engine.js";
import { SddError } from "../errors.js";
import { GitInspector } from "../git/git-inspector.js";
import { isCommandAllowed } from "../security/shell-policy.js";
import { validateTaskFiles } from "../security/task-scope.js";
import { FileLock } from "../state/file-lock.js";
import { StateStore } from "../state/state-store.js";

interface TaskResult extends TaskExecutionResult {
  taskId: string;
}

export async function runBuild(
  root: string,
  executor: TaskExecutor,
  signal?: AbortSignal,
  rawArgs?: Record<string, unknown>,
): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd build");
  const store = new StateStore(root);
  const activeTasks = new Set<string>();
  try {
    const state = await store.read();
    const retrying =
      state.currentPhase === "FAILED" && state.failedCommand === "sdd build";
    if (state.currentPhase !== "PLAN_READY" && !retrying) {
      throw new SddError(
        "E_INVALID_PHASE_COMMAND",
        `Cannot build from ${state.currentPhase}`,
        state.suggestedCommand ?? undefined,
      );
    }
    if (state.currentChangeId === null)
      throw new SddError("E_MISSING_CHANGE", "No active change");
    const changeId = state.currentChangeId;
    const change = join(root, ".sdd", "changes", changeId);
    const tasks = JSON.parse(
      await readFile(join(change, "tasks.json"), "utf8"),
    ) as TaskDefinition[];
    const previousResults = await readResults(
      join(change, "task-results.json"),
    );
    const results = [
      ...previousResults.filter(
        (result) => state.tasks[result.taskId] === "DONE",
      ),
    ];
    await store.update((current) => ({
      ...current,
      currentPhase: "BUILDING",
      inProgressPhase: "BUILDING",
      previousPhase: "PLAN_READY",
      lastCommand: "sdd build",
      lastError: null,
    }));

    const completed = new Set(
      Object.entries(state.tasks)
        .filter(([, status]) => status === "DONE")
        .map(([taskId]) => taskId),
    );
    const remaining = tasks.filter((task) => !completed.has(task.id));
    const git = new GitInspector(root);
    let gitBefore = await git.snapshot();
    while (remaining.length > 0) {
      const readyTasks = remaining.filter((task) =>
        task.dependsOn.every((dependency) => completed.has(dependency)),
      );
      if (readyTasks.length === 0)
        throw new SddError(
          "E_STATE_CORRUPTED",
          "Task dependency graph is cyclic or incomplete",
        );
      const batch = selectParallelBatch(readyTasks);
      for (const task of batch) {
        remaining.splice(
          remaining.findIndex((candidate) => candidate.id === task.id),
          1,
        );
        activeTasks.add(task.id);
      }
      await store.update((current) => ({
        ...current,
        tasks: {
          ...current.tasks,
          ...Object.fromEntries(batch.map((task) => [task.id, "BUILDING"])),
        },
      }));
      if (signal?.aborted === true) throw interruptionError();
      const executions = await Promise.all(
        batch.map(async (task) => {
          const contextPack = await readFile(
            join(root, ".sdd", "context-packs", changeId, `${task.id}.md`),
            "utf8",
          );
          const result = await executeWithLimits(
            executor,
            {
              root,
              task,
              contextPack,
              ...(signal === undefined ? {} : { signal }),
            },
            signal,
            timeoutMilliseconds(rawArgs),
          );
          return { task, result };
        }),
      );
      const gitAfter = await git.snapshot();
      const actualDelta = git.delta(gitBefore, gitAfter);
      assignUnreportedFiles(executions, actualDelta);
      for (const { task, result } of executions) {
        const modifiedFiles = [...new Set(result.modifiedFiles)];
        validateTaskFiles(modifiedFiles, task);
        for (const evidence of result.verification) {
          if (!isCommandAllowed(evidence.command)) {
            throw new SddError(
              "E_SECURITY_BLOCKED",
              `Verification command is not approved: ${evidence.command}`,
            );
          }
        }
        if (
          result.verification.length === 0 ||
          result.verification.some((evidence) => !evidence.passed)
        ) {
          throw new SddError(
            "E_VERIFY_FAILED",
            `Verification failed for ${task.id}`,
            "sdd build",
          );
        }
        const taskResult = { taskId: task.id, ...result, modifiedFiles };
        results.push(taskResult);
        completed.add(task.id);
        activeTasks.delete(task.id);
        const resultDirectory = join(
          root,
          ".sdd",
          "runs",
          state.currentRunId ?? "unknown-run",
          "tasks",
        );
        await mkdir(resultDirectory, { recursive: true });
        await writeFile(
          join(resultDirectory, `${task.id}.result.json`),
          `${JSON.stringify(taskResult, null, 2)}\n`,
          "utf8",
        );
      }
      await store.update((current) => ({
        ...current,
        tasks: {
          ...current.tasks,
          ...Object.fromEntries(batch.map((task) => [task.id, "DONE"])),
        },
      }));
      await writeFile(
        join(change, "task-results.json"),
        `${JSON.stringify(results, null, 2)}\n`,
        "utf8",
      );
      gitBefore = gitAfter;
    }

    const ready = await store.update((current) => ({
      ...current,
      currentPhase: "BUILD_READY",
      inProgressPhase: null,
      failedCommand: null,
      lastError: null,
      suggestedCommand: "sdd verify",
    }));
    await new AuditLogger(root).write({
      command: "sdd build",
      phase: ready.currentPhase,
      result: "PASS",
      changeId,
    });
    return {
      ok: true,
      state: ready.currentPhase,
      exitCode: 0,
      changeId,
      next: "sdd verify",
    };
  } catch (error) {
    if (error instanceof SddError) {
      await store.update((current) => ({
        ...current,
        currentPhase: error.exitCode === 130 ? "PAUSED" : "FAILED",
        inProgressPhase: null,
        failedCommand: error.exitCode === 130 ? null : "sdd build",
        interruptedCommand: error.exitCode === 130 ? "sdd build" : null,
        lastError: error.code,
        suggestedCommand: "sdd build",
        ...(activeTasks.size === 0
          ? {}
          : {
              tasks: {
                ...current.tasks,
                ...Object.fromEntries(
                  [...activeTasks].map((taskId) => [taskId, "FAILED" as const]),
                ),
              },
            }),
      }));
      await new AuditLogger(root).write({
        command: "sdd build",
        phase: error.exitCode === 130 ? "PAUSED" : "FAILED",
        result: "FAIL",
        message: error.code,
      });
    }
    throw error;
  } finally {
    await lock.release();
  }
}

function selectParallelBatch(tasks: TaskDefinition[]): TaskDefinition[] {
  const batch: TaskDefinition[] = [];
  for (const task of tasks) {
    if (batch.every((selected) => !taskScopesOverlap(task, selected)))
      batch.push(task);
  }
  return batch.length === 0 && tasks[0] !== undefined ? [tasks[0]] : batch;
}

function taskScopesOverlap(
  left: TaskDefinition,
  right: TaskDefinition,
): boolean {
  const leftPatterns = [...left.allowedFiles, ...left.expectedNewFiles];
  const rightPatterns = [...right.allowedFiles, ...right.expectedNewFiles];
  return leftPatterns.some((leftPattern) =>
    rightPatterns.some((rightPattern) => {
      const leftPrefix = staticPrefix(leftPattern);
      const rightPrefix = staticPrefix(rightPattern);
      return (
        leftPrefix === rightPrefix ||
        leftPrefix.startsWith(`${rightPrefix}/`) ||
        rightPrefix.startsWith(`${leftPrefix}/`)
      );
    }),
  );
}

function staticPrefix(pattern: string): string {
  return (
    pattern.replaceAll("\\", "/").split(/[*?]/, 1)[0]?.replace(/\/$/, "") ?? ""
  );
}

function assignUnreportedFiles(
  executions: Array<{ task: TaskDefinition; result: TaskExecutionResult }>,
  actualFiles: string[],
): void {
  const reported = new Set(
    executions.flatMap(({ result }) => result.modifiedFiles),
  );
  for (const file of actualFiles.filter(
    (candidate) => !reported.has(candidate),
  )) {
    const owners = executions.filter(({ task }) => fileFitsTask(file, task));
    if (owners.length === 0) {
      throw new SddError(
        "E_SECURITY_BLOCKED",
        `Unreported file is outside task scope: ${file}`,
      );
    }
    if (owners.length > 1) {
      throw new SddError(
        "E_PARALLEL_FILE_CONFLICT",
        `File can be attributed to multiple parallel tasks: ${file}`,
      );
    }
    owners[0]?.result.modifiedFiles.push(file);
  }
}

function fileFitsTask(file: string, task: TaskDefinition): boolean {
  try {
    validateTaskFiles([file], task);
    return true;
  } catch {
    return false;
  }
}

async function readResults(path: string): Promise<TaskResult[]> {
  try {
    return JSON.parse(await readFile(path, "utf8")) as TaskResult[];
  } catch {
    return [];
  }
}

function interruptionError(): SddError {
  return new SddError("E_INTERRUPTED", "Build interrupted", "sdd build");
}

function timeoutMilliseconds(
  args: Record<string, unknown> | undefined,
): number | undefined {
  const seconds = args?.timeout;
  return typeof seconds === "number" && seconds > 0
    ? seconds * 1_000
    : undefined;
}

async function executeWithLimits(
  executor: TaskExecutor,
  request: Parameters<TaskExecutor["execute"]>[0],
  signal: AbortSignal | undefined,
  timeout: number | undefined,
): Promise<TaskExecutionResult> {
  const promises: Array<Promise<TaskExecutionResult>> = [
    executor.execute(request),
  ];
  if (timeout !== undefined) {
    promises.push(
      new Promise((_, reject) => {
        setTimeout(
          () =>
            reject(
              new SddError(
                "E_TIMEOUT",
                `Build task timed out after ${timeout}ms`,
                "sdd build",
              ),
            ),
          timeout,
        );
      }),
    );
  }
  if (signal !== undefined) {
    promises.push(
      new Promise((_, reject) => {
        if (signal.aborted) reject(interruptionError());
        else
          signal.addEventListener("abort", () => reject(interruptionError()), {
            once: true,
          });
      }),
    );
  }
  return Promise.race(promises);
}
