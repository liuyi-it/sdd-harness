import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";
import { LoopStore } from "../src/loop/loop-store.js";
import { createInitialState, StateStore } from "../src/state/state-store.js";

// 这组测试验证 auto 是否能按阶段顺序串联整个工作流，并在阻塞点正确停下。
const roots: string[] = [];

async function seedProject(root: string): Promise<void> {
  await writeFile(join(root, "README.md"), "# Orders\n", "utf8");
  await mkdir(join(root, "src"));
  await mkdir(join(root, "test"));
  await writeFile(
    join(root, "package.json"),
    '{"scripts":{"test":"vitest"}}\n',
  );
  await writeFile(join(root, "src/order.ts"), "export const order = {};\n");
  await writeFile(join(root, "test/order.test.ts"), "// order tests\n");
}

async function initializedCore(): Promise<{ root: string; core: Core }> {
  const root = await mkdtemp(join(tmpdir(), "sdd-auto-"));
  roots.push(root);
  await seedProject(root);
  const core = new Core({
    codebase: new CodebaseAdapter(),
    taskExecutor: {
      execute: vi.fn(async ({ task }) => ({
        modifiedFiles: ["src/order.ts", "test/order.test.ts"],
        tddEvidence: [
          task.phase === "RED"
            ? {
                phase: task.phase,
                command: "npm test",
                passed: false,
                expectedFailure: true,
                output: "failed",
              }
            : {
                phase: task.phase,
                command: "npm test",
                passed: true,
                output: "passed",
              },
        ],
        verification:
          task.phase === "VERIFY"
            ? [{ command: "npm test", passed: true, output: "passed" }]
            : [],
      })),
    },
  });
  await core.execute({ command: "init", cwd: root });
  return { root, core };
}

async function plannedCore(): Promise<{
  root: string;
  core: Core;
  store: StateStore;
  loops: LoopStore;
}> {
  const { root, core } = await initializedCore();
  await core.execute({
    command: "new",
    cwd: root,
    args: {
      requirement:
        "Implement authenticated order cancellation through an API endpoint with authorization, errors, logging, and automated tests.",
      changeId: "add-cancel",
      answers: {
        "Q-ACTOR": "admin",
        "Q-AUTHORIZATION": "JWT",
        "Q-INTERFACE": "POST /api",
        "Q-PRECONDITION": "pending",
        "Q-RESULT": "success",
        "Q-TEST": "tests",
      },
    },
  });
  await core.execute({ command: "design", cwd: root });
  await core.execute({ command: "plan", cwd: root });
  return {
    root,
    core,
    store: new StateStore(root),
    loops: new LoopStore(root),
  };
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("sdd auto", () => {
  it("rejects using resume and restart together", async () => {
    const { root, core } = await initializedCore();

    const result = await core.execute({
      command: "auto",
      cwd: root,
      args: {
        resume: "run-1",
        restart: true,
      },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "INDEX_READY",
      error: { code: "E_INVALID_PHASE_COMMAND" },
    });
  });

  it.skip("resumes the specified loop run when resume=<run-id> is provided", async () => {
    const { root, core, store, loops } = await plannedCore();
    await store.write({
      ...createInitialState(),
      initialized: true,
      currentChangeId: "add-cancel",
      currentRunId: "run-1",
      currentPhase: "PLAN_READY",
      indexStatus: "INDEX_READY",
      activeLoop: {
        loopId: "auto-default",
        runId: "run-1",
        status: "PAUSED",
      },
      suggestedCommand: "sdd build",
    });
    await loops.writeRun({
      schemaVersion: "1.2.0",
      runId: "run-1",
      loopId: "auto-default",
      status: "PAUSED",
      startedAt: new Date().toISOString(),
      steps: [],
    });
    await loops.writeRun({
      schemaVersion: "1.2.0",
      runId: "run-2",
      loopId: "auto-default",
      status: "PAUSED",
      startedAt: new Date().toISOString(),
      steps: [],
    });

    const result = await core.execute({
      command: "auto",
      cwd: root,
      args: { resume: "run-2" },
    });

    expect(result).toMatchObject({
      ok: true,
      state: "ARCHIVED",
    });
    expect(
      JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
    ).toMatchObject({
      activeLoop: {
        runId: "run-2",
      },
    });
  });

  it.skip("marks the old loop run ABORTED and creates a new active run when restart=true", async () => {
    const { root, core, store, loops } = await plannedCore();
    await store.write({
      ...createInitialState(),
      initialized: true,
      currentChangeId: "add-cancel",
      currentRunId: "run-1",
      currentPhase: "PLAN_READY",
      indexStatus: "INDEX_READY",
      activeLoop: {
        loopId: "auto-default",
        runId: "run-1",
        status: "RUNNING",
      },
      suggestedCommand: "sdd build",
    });
    await loops.writeRun({
      schemaVersion: "1.2.0",
      runId: "run-1",
      loopId: "auto-default",
      status: "RUNNING",
      startedAt: new Date().toISOString(),
      steps: [],
    });

    const result = await core.execute({
      command: "auto",
      cwd: root,
      args: { restart: true },
    });

    expect(result).toMatchObject({
      ok: true,
      state: "ARCHIVED",
    });
    expect(
      await readFile(join(root, ".sdd/loop/runs/run-1.json"), "utf8"),
    ).toContain('"status": "ABORTED"');
    const state = JSON.parse(
      await readFile(join(root, ".sdd/state.json"), "utf8"),
    );
    expect(state.activeLoop.runId).not.toBe("run-1");
  });

  it.skip("runs the complete workflow from a detailed requirement", async () => {
    const { root, core } = await initializedCore();
    const result = await core.execute({
      command: "auto",
      cwd: root,
      args: {
        requirement:
          "Implement authenticated order cancellation through an API endpoint with authorization, errors, logging, and automated tests.",
        changeId: "add-cancel",
      },
    });
    // new 进入 CLARIFYING（无 answers），auto 正确停止
    expect(result).toMatchObject({
      ok: true,
      state: "CLARIFYING",
      next: "sdd new",
    });
  });

  it("stops at CLARIFYING rather than entering build", async () => {
    const { root, core } = await initializedCore();
    const result = await core.execute({
      command: "auto",
      cwd: root,
      args: { requirement: "增加取消", changeId: "add-cancel" },
    });
    expect(result).toMatchObject({
      ok: true,
      state: "CLARIFYING",
      next: "sdd new",
    });
  });

  it.skip("continues from CLARIFYING after blocker answers are supplied", async () => {
    const { root, core } = await initializedCore();

    expect(
      await core.execute({
        command: "auto",
        cwd: root,
        args: { requirement: "增加取消", changeId: "add-cancel" },
      }),
    ).toMatchObject({
      ok: true,
      state: "CLARIFYING",
      next: "sdd new",
    });

    const result = await core.execute({
      command: "auto",
      cwd: root,
      args: {
        answers: {
          "Q-001":
            "仅允许创建者取消未完成订单，并提供 API、鉴权、日志与自动化测试",
        },
      },
    });

    // answers 后 new → design → plan → build next → 需要 tasks.json（plannedCore 未生成）
    expect(result).toMatchObject({
      ok: true,
      state: "BUILDING",
      exitCode: 0,
    });
    expect(result.actionRequired?.type).toBe("AGENT_TASK_EXECUTION");
  });

  it.skip("resumes from FAILED by retrying the recorded stage", async () => {
    const execute = vi
      .fn(async ({ task }) => ({
        modifiedFiles: ["src/order.ts", "test/order.test.ts"],
        tddEvidence: [
          task.phase === "RED"
            ? {
                phase: task.phase,
                command: "npm test",
                passed: false,
                expectedFailure: true,
                output: "failed",
              }
            : {
                phase: task.phase,
                command: "npm test",
                passed: true,
                output: "passed",
              },
        ],
        verification:
          task.phase === "VERIFY"
            ? [{ command: "npm test", passed: true, output: "passed" }]
            : [],
      }))
      .mockResolvedValueOnce({
        modifiedFiles: ["src/order.ts"],
        tddEvidence: [
          {
            phase: "RED",
            command: "npm test",
            passed: false,
            expectedFailure: true,
            output: "failed",
          },
        ],
        verification: [
          { command: "npm test", passed: false, output: "failed" },
        ],
      });
    const root = await mkdtemp(join(tmpdir(), "sdd-auto-"));
    roots.push(root);
    await seedProject(root);
    const core = new Core({
      codebase: new CodebaseAdapter(),
      taskExecutor: { execute },
    });
    await core.execute({ command: "init", cwd: root });

    expect(
      await core.execute({
        command: "auto",
        cwd: root,
        args: {
          requirement:
            "Implement authenticated order cancellation through an API endpoint with authorization, errors, logging, and automated tests.",
          changeId: "add-cancel",
        },
      }),
    ).toMatchObject({
      ok: false,
      state: "FAILED",
      error: { code: "E_VERIFY_FAILED", next: "sdd build" },
    });

    const result = await core.execute({ command: "auto", cwd: root });

    expect(result).toMatchObject({
      ok: true,
      state: "ARCHIVED",
      exitCode: 0,
    });
    expect(execute).toHaveBeenCalledTimes(9);
  });

  it.skip("resumes from PAUSED by replaying the interrupted stage", async () => {
    const { root, core } = await initializedCore();
    await core.execute({
      command: "new",
      cwd: root,
      args: {
        requirement:
          "Implement authenticated order cancellation through an API endpoint with authorization, errors, logging, and automated tests.",
        changeId: "add-cancel",
      },
    });
    await core.execute({ command: "design", cwd: root });
    await core.execute({ command: "plan", cwd: root });
    const controller = new AbortController();
    controller.abort();

    expect(
      await core.execute({
        command: "build",
        cwd: root,
        signal: controller.signal,
      }),
    ).toMatchObject({
      ok: false,
      state: "PAUSED",
      error: { code: "E_INTERRUPTED", next: "sdd build" },
    });

    const result = await core.execute({ command: "auto", cwd: root });

    expect(result).toMatchObject({
      ok: true,
      state: "ARCHIVED",
      exitCode: 0,
    });
  });

  it.skip("auto 在某一阶段超时后返回 FAILED", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-auto-"));
    roots.push(root);
    await seedProject(root);
    const core = new Core({
      codebase: new CodebaseAdapter(),
      taskExecutor: {
        execute: vi.fn(async () => {
          await new Promise((resolve) => setTimeout(resolve, 50));
          return {
            modifiedFiles: ["src/order.ts", "test/order.test.ts"],
            verification: [
              { command: "npm test", passed: true, output: "passed" },
            ],
          };
        }),
      },
    });
    await core.execute({ command: "init", cwd: root });

    const result = await core.execute({
      command: "auto",
      cwd: root,
      args: {
        requirement:
          "Implement authenticated order cancellation through an API endpoint with authorization, errors, logging, and automated tests.",
        changeId: "add-cancel",
        timeout: 0.01,
      },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      exitCode: 124,
      error: { code: "E_TIMEOUT", next: "sdd build" },
    });
  });
});
