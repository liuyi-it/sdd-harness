import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  writeFile,
} from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";
import { LoopStore } from "../src/loop/loop-store.js";
import { createInitialState, StateStore } from "../src/state/state-store.js";

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
  execFileSync("git", ["init", "-b", "main"], { cwd: root });
  execFileSync("git", ["config", "user.email", "test@example.com"], {
    cwd: root,
  });
  execFileSync("git", ["config", "user.name", "SDD Harness Test"], {
    cwd: root,
  });
  execFileSync("git", ["add", "."], { cwd: root });
  execFileSync("git", ["commit", "-m", "init"], { cwd: root });
}

/** 基础 Core：只有 init，没有 new/design/plan 制品 */
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

/** 完整制品 Core：init → new(answers) → design → plan，plan.json 已就绪 */
async function plannedCore(): Promise<{
  root: string;
  core: Core;
  store: StateStore;
  loops: LoopStore;
}> {
  const { root, core } = await initializedCore();
  const answers = {
    "Q-ACTOR": "admin",
    "Q-AUTHORIZATION": "JWT",
    "Q-INTERFACE": "POST /api",
    "Q-PRECONDITION": "pending",
    "Q-RESULT": "success",
    "Q-TEST": "tests",
  };
  await core.execute({
    command: "new",
    cwd: root,
    args: {
      requirement:
        "Implement authenticated order cancellation through an API endpoint with authorization, errors, logging, and automated tests.",
      changeId: "add-cancel",
      answers,
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
  it("auto 与手动 build next 生成语义等价的 Policy、任务和 Context Pack", async () => {
    const manual = await plannedCore();
    const automatic = await plannedCore();

    const manualResult = await manual.core.execute({
      command: "build",
      cwd: manual.root,
      args: { subcommand: "next" },
    });
    const autoResult = await automatic.core.execute({
      command: "auto",
      cwd: automatic.root,
    });

    expect(autoResult).toMatchObject({
      ok: true,
      state: "BUILD_WAITING_AGENT",
    });
    expect(stripRunSpecific(autoResult.actionRequired)).toEqual(
      stripRunSpecific(manualResult.actionRequired),
    );
    const manualTasks = JSON.parse(
      await readFile(
        join(manual.root, ".sdd/changes/add-cancel/plan.json"),
        "utf8",
      ),
    ).tasks;
    const autoTasks = JSON.parse(
      await readFile(
        join(automatic.root, ".sdd/changes/add-cancel/plan.json"),
        "utf8",
      ),
    ).tasks;
    expect(autoTasks).toEqual(manualTasks);
    const contextPath = manualResult.actionRequired?.contextPack;
    expect(contextPath).toBe(autoResult.actionRequired?.contextPack);
    expect(
      normalizeContextPack(
        await readFile(join(automatic.root, contextPath!), "utf8"),
      ),
    ).toBe(
      normalizeContextPack(
        await readFile(join(manual.root, contextPath!), "utf8"),
      ),
    );
  }, 30_000);

  it("auto 不创建 worktree、提交、分支或外部 ticket", async () => {
    const { root, core } = await plannedCore();
    const headBefore = execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: root,
      encoding: "utf8",
    }).trim();

    await core.execute({ command: "auto", cwd: root });

    expect(
      execFileSync("git", ["rev-parse", "HEAD"], {
        cwd: root,
        encoding: "utf8",
      }).trim(),
    ).toBe(headBefore);
    expect(
      execFileSync("git", ["branch", "--format=%(refname:short)"], {
        cwd: root,
        encoding: "utf8",
      }).trim(),
    ).toBe("main");
    await expect(access(join(root, ".worktrees"))).rejects.toThrow();
    const repositoryFiles = await readdir(root);
    expect(
      repositoryFiles.some((name) => /(?:ticket|issue)/iu.test(name)),
    ).toBe(false);
  });

  it("rejects using resume and restart together", async () => {
    const { root, core } = await initializedCore();
    const result = await core.execute({
      command: "auto",
      cwd: root,
      args: { resume: "run-1", restart: true },
    });
    expect(result).toMatchObject({
      ok: false,
      state: "INDEX_READY",
      error: { code: "E_INVALID_PHASE_COMMAND" },
    });
  });

  // resume 测试：plan 已生成 plan.json，Agent 循环执行全部任务后到 ARCHIVED
  (process.platform === "win32" ? it.skip : it)(
    "resumes the specified loop run when resume=<run-id> is provided",
    async () => {
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
        tasks: {},
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
      expect(result).toMatchObject({ ok: true, state: "BUILD_WAITING_AGENT" });
      expect(result.actionRequired?.type).toBe("AGENT_TASK_EXECUTION");
      expect(
        JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
      ).toMatchObject({ activeLoop: { runId: "run-2" } });
    },
  );

  // restart 测试：plan 已生成 plan.json，Agent 循环执行全部任务后到 ARCHIVED
  (process.platform === "win32" ? it.skip : it)(
    "marks the old loop run ABORTED and creates a new active run when restart=true",
    async () => {
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
        tasks: {},
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
      expect(result).toMatchObject({ ok: true, state: "BUILD_WAITING_AGENT" });
      expect(result.actionRequired?.type).toBe("AGENT_TASK_EXECUTION");
      expect(
        await readFile(join(root, ".sdd/loop/runs/run-1.json"), "utf8"),
      ).toContain('"status": "ABORTED"');
      const state = JSON.parse(
        await readFile(join(root, ".sdd/state.json"), "utf8"),
      );
      expect(state.activeLoop.runId).not.toBe("run-1");
    },
  );

  it("restart 会撤销旧 pending handoff 并为新 run 重新分配结果路径", async () => {
    const { root, core } = await plannedCore();
    const first = await core.execute({ command: "auto", cwd: root });
    const oldResultFile = first.actionRequired?.resultFile;

    const restarted = await core.execute({
      command: "auto",
      cwd: root,
      args: { restart: true },
    });

    expect(restarted).toMatchObject({
      ok: true,
      state: "BUILD_WAITING_AGENT",
      actionRequired: { type: "AGENT_TASK_EXECUTION" },
    });
    expect(restarted.actionRequired?.resultFile).not.toBe(oldResultFile);
    const state = await new StateStore(root).read();
    expect(state.pendingAgentTask?.resultFile).toBe(
      restarted.actionRequired?.resultFile,
    );
    expect(state.pendingAgentTask?.resultFile).not.toContain("run-1/");
  });

  it("stop 会清除 pending handoff 并把 BUILDING 任务恢复为 PENDING", async () => {
    const { root, core } = await plannedCore();
    const first = await core.execute({ command: "auto", cwd: root });
    const taskId = first.actionRequired!.taskId;

    await expect(
      core.execute({ command: "auto", cwd: root, args: { stop: true } }),
    ).resolves.toMatchObject({ ok: true, state: "PLAN_READY" });
    const state = await new StateStore(root).read();
    expect(state.pendingAgentTask).toBeNull();
    expect(state.tasks[taskId]).toBe("PENDING");
    expect(state.activeLoop?.status).toBe("ABORTED");
  });

  // 无 answers → new 进入 CLARIFYING → auto 正确停止
  (process.platform === "win32" ? it.skip : it)(
    "runs the complete workflow from a detailed requirement",
    async () => {
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
      expect(result).toMatchObject({
        ok: true,
        state: "BUILD_WAITING_AGENT",
        exitCode: 0,
      });
      expect(result.actionRequired?.type).toBe("AGENT_TASK_EXECUTION");
    },
  );

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
    expect(result.data).toMatchObject({
      clarification: {
        questions: expect.arrayContaining([
          {
            id: "Q-ACTOR",
            question: "请明确谁是执行该行为的业务角色。",
          },
        ]),
      },
    });
  });

  // 有 answers → 绕过 CLARIFYING → 推进到 BUILDING
  (process.platform === "win32" ? it.skip : it)(
    "continues from CLARIFYING after blocker answers are supplied",
    async () => {
      const { root, core } = await initializedCore();
      expect(
        await core.execute({
          command: "auto",
          cwd: root,
          args: { requirement: "增加取消", changeId: "add-cancel" },
        }),
      ).toMatchObject({ ok: true, state: "CLARIFYING", next: "sdd new" });
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
      expect(result).toMatchObject({
        ok: true,
        state: "BUILD_WAITING_AGENT",
        exitCode: 0,
      });
      expect(result.actionRequired?.type).toBe("AGENT_TASK_EXECUTION");
    },
  );
});

function stripRunSpecific(action: unknown): unknown {
  if (typeof action !== "object" || action === null) return action;
  const stable = { ...(action as Record<string, unknown>) };
  delete stable.resultFile;
  return stable;
}

function normalizeContextPack(content: string): string {
  return content.replace(/^Generated At: .*$/gmu, "Generated At: <normalized>");
}
