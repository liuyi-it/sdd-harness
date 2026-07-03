import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";
import { type TaskExecutor } from "../src/build/task-executor.js";

// build 是行为最复杂的阶段之一，因此这里集中覆盖成功、失败、重试、暂停、超时和并行执行。
const roots: string[] = [];

async function plannedProject(
  executor: TaskExecutor,
): Promise<{ root: string; core: Core }> {
  const root = await mkdtemp(join(tmpdir(), "sdd-build-"));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Orders\n", "utf8");
  const core = new Core({
    codebase: new CodebaseAdapter(),
    taskExecutor: executor,
  });
  await core.execute({ command: "init", cwd: root });
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
  return { root, core };
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("sdd build", () => {
  it("executes pending tasks and records verification evidence", async () => {
    const execute = vi.fn().mockResolvedValue({
      modifiedFiles: ["src/order.ts", "test/order.test.ts"],
      verification: [{ command: "npm test", passed: true, output: "1 passed" }],
    });
    const { root, core } = await plannedProject({ execute });

    const result = await core.execute({ command: "build", cwd: root });

    expect(result).toMatchObject({
      ok: true,
      state: "BUILD_READY",
      next: "sdd verify",
    });
    expect(execute).toHaveBeenCalledTimes(1);
    const state = JSON.parse(
      await readFile(join(root, ".sdd/state.json"), "utf8"),
    );
    expect(state.tasks).toEqual({ "TASK-001": "DONE" });
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/changes/add-cancel/task-results.json"),
          "utf8",
        ),
      ),
    ).toMatchObject([{ taskId: "TASK-001", verification: [{ passed: true }] }]);
  });

  it("blocks files outside the task scope", async () => {
    const { root, core } = await plannedProject({
      execute: vi.fn().mockResolvedValue({
        modifiedFiles: ["secrets.txt"],
        verification: [{ command: "npm test", passed: true, output: "pass" }],
      }),
    });

    const result = await core.execute({ command: "build", cwd: root });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      exitCode: 10,
      error: { code: "E_SECURITY_BLOCKED" },
    });
  });

  it("marks a failed task and can retry only failed tasks", async () => {
    const execute = vi
      .fn()
      .mockResolvedValueOnce({
        modifiedFiles: ["src/order.ts"],
        verification: [
          { command: "npm test", passed: false, output: "failed" },
        ],
      })
      .mockResolvedValueOnce({
        modifiedFiles: ["src/order.ts"],
        verification: [{ command: "npm test", passed: true, output: "passed" }],
      });
    const { root, core } = await plannedProject({ execute });

    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: false,
      state: "FAILED",
    });
    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: true,
      state: "BUILD_READY",
    });
    expect(execute).toHaveBeenCalledTimes(2);
  });

  it("moves to PAUSED with exit code 130 when cancelled", async () => {
    const { root, core } = await plannedProject({ execute: vi.fn() });
    const controller = new AbortController();
    controller.abort();

    const result = await core.execute({
      command: "build",
      cwd: root,
      signal: controller.signal,
    });

    expect(result).toMatchObject({
      ok: false,
      state: "PAUSED",
      exitCode: 130,
      error: { code: "E_INTERRUPTED", next: "sdd build" },
    });
  });

  it("moves to FAILED with E_TIMEOUT when the task executor exceeds timeout", async () => {
    const { root, core } = await plannedProject({
      execute: vi.fn(() => new Promise(() => undefined)),
    });

    const result = await core.execute({
      command: "build",
      cwd: root,
      args: { timeout: 0.01 },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      exitCode: 124,
      error: { code: "E_TIMEOUT", next: "sdd build" },
    });
  });

  it("runs independent non-overlapping tasks in parallel and stores isolated results", async () => {
    let active = 0;
    let peak = 0;
    const execute = vi.fn(async ({ task }: { task: { id: string } }) => {
      active += 1;
      peak = Math.max(peak, active);
      await new Promise((resolve) => setTimeout(resolve, 20));
      active -= 1;
      const area = task.id === "TASK-001" ? "orders" : "audit";
      return {
        modifiedFiles: [`src/${area}/index.ts`],
        verification: [{ command: "npm test", passed: true, output: "passed" }],
      };
    });
    const { root, core } = await plannedProject({ execute });
    const change = join(root, ".sdd/changes/add-cancel");
    const tasks = [
      {
        id: "TASK-001",
        title: "Orders",
        status: "PENDING",
        requirements: ["REQ-001"],
        dependsOn: [],
        allowedFiles: ["src/orders/**"],
        expectedNewFiles: ["src/orders/**"],
        forbiddenFiles: [".git/**"],
        verification: ["npm test"],
        doneCriteria: ["done"],
      },
      {
        id: "TASK-002",
        title: "Audit",
        status: "PENDING",
        requirements: ["REQ-001"],
        dependsOn: [],
        allowedFiles: ["src/audit/**"],
        expectedNewFiles: ["src/audit/**"],
        forbiddenFiles: [".git/**"],
        verification: ["npm test"],
        doneCriteria: ["done"],
      },
    ];
    await writeFile(
      join(change, "tasks.json"),
      `${JSON.stringify(tasks)}\n`,
      "utf8",
    );
    await writeFile(
      join(root, ".sdd/context-packs/add-cancel/TASK-002.md"),
      "# TASK-002",
      "utf8",
    );
    const statePath = join(root, ".sdd/state.json");
    const state = JSON.parse(await readFile(statePath, "utf8"));
    state.tasks = { "TASK-001": "PENDING", "TASK-002": "PENDING" };
    await writeFile(statePath, `${JSON.stringify(state)}\n`, "utf8");

    const result = await core.execute({ command: "build", cwd: root });

    expect(result.state).toBe("BUILD_READY");
    expect(peak).toBe(2);
    const runId = JSON.parse(await readFile(statePath, "utf8")).currentRunId;
    for (const task of tasks) {
      await expect(
        readFile(
          join(root, ".sdd/runs", runId, "tasks", `${task.id}.result.json`),
          "utf8",
        ),
      ).resolves.toContain(task.id);
    }
  });
});
