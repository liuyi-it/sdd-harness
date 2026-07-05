import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import { artifactInputHash } from "../src/artifacts/artifact-writer.js";
import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";
import { type TaskExecutor } from "../src/build/task-executor.js";
import { createInitialState, StateStore } from "../src/state/state-store.js";

function evidenceFor(phase: "RED" | "GREEN" | "REFACTOR" | "VERIFY") {
  return {
    tddEvidence: [
      phase === "RED"
        ? {
            phase,
            command: "npm test",
            passed: false,
            expectedFailure: true,
            output: "1 failed",
          }
        : { phase, command: "npm test", passed: true, output: "1 passed" },
    ],
    verification:
      phase === "VERIFY"
        ? [{ command: "npm test", passed: true, output: "1 passed" }]
        : [],
  };
}

// build 是行为最复杂的阶段之一，因此这里集中覆盖成功、失败、重试、暂停、超时和并行执行。
const roots: string[] = [];

async function plannedProject(
  executor: TaskExecutor,
): Promise<{ root: string; core: Core }> {
  const root = await mkdtemp(join(tmpdir(), "sdd-build-"));
  roots.push(root);
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
  const core = new Core({
    codebase: new CodebaseAdapter(),
    taskExecutor: {
      execute: async (request) => {
        const result = await executor.execute(request);
        return {
          ...result,
          ...(result.tddEvidence === undefined
            ? { tddEvidence: evidenceFor(request.task.phase).tddEvidence }
            : {}),
        };
      },
    },
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
  it("缺少 TDD 证据时拒绝完成任务", async () => {
    const { core, root } = await plannedProject({
      execute: vi
        .fn()
        .mockResolvedValue({ modifiedFiles: [], verification: [] }),
    });

    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: false,
      exitCode: 7,
      error: { code: "E_TDD_EVIDENCE_REQUIRED" },
    });
  });

  it.each([
    [
      "RED 证据错误地通过",
      {
        phase: "RED",
        command: "npm test",
        passed: true,
        expectedFailure: true,
        output: "pass",
      },
    ],
    [
      "RED 证据未标记预期失败",
      { phase: "RED", command: "npm test", passed: false, output: "fail" },
    ],
    [
      "证据阶段与任务不匹配",
      { phase: "GREEN", command: "npm test", passed: true, output: "pass" },
    ],
  ])("%s 时拒绝完成任务", async (_name, evidence) => {
    const { core, root } = await plannedProject({
      execute: vi.fn().mockResolvedValue({
        modifiedFiles: [],
        tddEvidence: [evidence],
        verification: [],
      }),
    });

    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: false,
      error: { code: "E_TDD_EVIDENCE_REQUIRED" },
    });
  });

  it.each(["GREEN", "REFACTOR", "VERIFY"] as const)(
    "%s 证据显式携带 expectedFailure=false 时拒绝",
    async (phase) => {
      const execute = vi.fn(async ({ task }) => ({
        modifiedFiles: [],
        ...(task.phase === phase
          ? {
              tddEvidence: [
                {
                  phase: task.phase,
                  command: "npm test",
                  passed: true,
                  expectedFailure: false,
                  output: "pass",
                },
              ],
              verification:
                task.phase === "VERIFY"
                  ? [{ command: "npm test", passed: true, output: "pass" }]
                  : [],
            }
          : evidenceFor(task.phase)),
      }));
      const { core, root } = await plannedProject({ execute });
      expect(await core.execute({ command: "build", cwd: root })).toMatchObject(
        {
          ok: false,
          error: { code: "E_TDD_EVIDENCE_REQUIRED" },
        },
      );
    },
  );

  it.each(["GREEN", "REFACTOR", "VERIFY"] as const)(
    "%s passed=false 时拒绝",
    async (phase) => {
      const { core, root } = await plannedProject({
        execute: vi.fn(async ({ task }) => ({
          modifiedFiles: [],
          ...(task.phase === phase
            ? {
                tddEvidence: [
                  {
                    phase,
                    command: "npm test",
                    passed: false,
                    output: "failed",
                  },
                ],
                verification:
                  phase === "VERIFY"
                    ? [{ command: "npm test", passed: true, output: "pass" }]
                    : [],
              }
            : evidenceFor(task.phase)),
        })),
      });
      expect(await core.execute({ command: "build", cwd: root })).toMatchObject(
        { ok: false, error: { code: "E_TDD_EVIDENCE_REQUIRED" } },
      );
    },
  );

  it("VERIFY 缺少 verification 时拒绝", async () => {
    const { core, root } = await plannedProject({
      execute: vi.fn(async ({ task }) => ({
        modifiedFiles: [],
        ...evidenceFor(task.phase),
        ...(task.phase === "VERIFY" ? { verification: [] } : {}),
      })),
    });
    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: false,
      error: { code: "E_TDD_EVIDENCE_REQUIRED" },
    });
  });

  it("TDD 证据命令越权时安全阻断", async () => {
    const { core, root } = await plannedProject({
      execute: vi.fn(async ({ task }) => ({
        modifiedFiles: [],
        ...evidenceFor(task.phase),
        tddEvidence: [
          { ...evidenceFor(task.phase).tddEvidence[0], command: "rm -rf /" },
        ],
      })),
    });
    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: false,
      error: { code: "E_SECURITY_BLOCKED" },
    });
  });

  it("畸形 TDD 证据安全失败而不崩溃", async () => {
    const { core, root } = await plannedProject({
      execute: vi.fn().mockResolvedValue({
        modifiedFiles: [],
        tddEvidence: [{ phase: "RED", command: 1, passed: "no", output: null }],
        verification: [],
      }),
    });
    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: false,
      error: { code: "E_TDD_EVIDENCE_REQUIRED" },
    });
  });

  it("持久化各阶段真实证据并允许完整链完成", async () => {
    const execute = vi.fn(async ({ task }) => ({
      modifiedFiles: ["src/order.ts", "test/order.test.ts"],
      ...evidenceFor(task.phase),
    }));
    const { core, root } = await plannedProject({ execute });

    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: true,
      state: "BUILD_READY",
    });
    const results = JSON.parse(
      await readFile(
        join(root, ".sdd/changes/add-cancel/task-results.json"),
        "utf8",
      ),
    );
    expect(results[0].tddEvidence[0]).toMatchObject({
      phase: "RED",
      passed: false,
      expectedFailure: true,
    });
    expect(results.at(-1).tddEvidence[0]).toMatchObject({
      phase: "VERIFY",
      passed: true,
    });
  });
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
    expect(execute).toHaveBeenCalledTimes(8);
    const state = JSON.parse(
      await readFile(join(root, ".sdd/state.json"), "utf8"),
    );
    expect(Object.keys(state.tasks)).toHaveLength(8);
    expect(Object.values(state.tasks)).toEqual(
      expect.arrayContaining(["DONE"]),
    );
    const taskResults = JSON.parse(
      await readFile(
        join(root, ".sdd/changes/add-cancel/task-results.json"),
        "utf8",
      ),
    );
    expect(taskResults).toHaveLength(8);
    expect(taskResults[0]).toMatchObject({
      taskId: "TASK-001-RED",
      verification: [{ passed: true }],
    });
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/changes/add-cancel/git-baseline.json"),
          "utf8",
        ),
      ),
    ).toMatchObject({
      available: true,
      files: expect.any(Array),
    });
  }, 15_000);

  it("warns when build starts with pre-existing uncommitted changes", async () => {
    const execute = vi.fn().mockResolvedValue({
      modifiedFiles: ["src/order.ts", "test/order.test.ts"],
      verification: [{ command: "npm test", passed: true, output: "1 passed" }],
    });
    const { root, core } = await plannedProject({ execute });
    await writeFile(join(root, "README.md"), "# Orders changed\n", "utf8");

    const result = await core.execute({ command: "build", cwd: root });

    expect(result.warnings).toEqual(
      expect.arrayContaining([
        expect.stringContaining("检测到执行前已有未提交修改"),
      ]),
    );
  });

  it("当 --change 与当前活动变更不一致时拒绝执行 build", async () => {
    const execute = vi.fn().mockResolvedValue({
      modifiedFiles: ["src/order.ts", "test/order.test.ts"],
      verification: [{ command: "npm test", passed: true, output: "1 passed" }],
    });
    const { root, core } = await plannedProject({ execute });

    const result = await core.execute({
      command: "build",
      cwd: root,
      args: { changeId: "other-change" },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      error: { code: "E_MISSING_CHANGE" },
    });
    expect(execute).not.toHaveBeenCalled();
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

  it("当 spec/design/tasks 变化导致 Context Pack 失效时要求重新执行 plan", async () => {
    const execute = vi.fn().mockResolvedValue({
      modifiedFiles: ["src/order.ts"],
      verification: [{ command: "npm test", passed: true, output: "passed" }],
    });
    const { root, core } = await plannedProject({ execute });
    await writeFile(
      join(root, ".sdd/changes/add-cancel/design.md"),
      "# 用户修改后的设计\n",
      "utf8",
    );

    const result = await core.execute({ command: "build", cwd: root });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      error: {
        code: "E_MISSING_ARTIFACT",
        next: "sdd plan",
      },
    });
    expect(execute).not.toHaveBeenCalled();
  });

  it("当代码库索引变化导致 Context Pack 失效时要求重新执行 plan", async () => {
    const execute = vi.fn().mockResolvedValue({
      modifiedFiles: ["src/order.ts"],
      verification: [{ command: "npm test", passed: true, output: "passed" }],
    });
    const { root, core } = await plannedProject({ execute });
    await writeFile(
      join(root, ".sdd/index/codebase-summary.md"),
      "# Updated index summary\n",
      "utf8",
    );

    const result = await core.execute({ command: "build", cwd: root });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      error: {
        code: "E_MISSING_ARTIFACT",
        next: "sdd plan",
      },
    });
    expect(execute).not.toHaveBeenCalled();
  });

  it("marks a failed task and can retry only failed tasks", async () => {
    const execute = vi
      .fn()
      .mockResolvedValue({
        modifiedFiles: ["src/order.ts", "test/order.test.ts"],
        verification: [{ command: "npm test", passed: true, output: "passed" }],
      })
      .mockResolvedValueOnce({
        modifiedFiles: ["src/order.ts"],
        verification: [
          { command: "npm test", passed: false, output: "failed" },
        ],
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
    expect(execute).toHaveBeenCalledTimes(9);
  });

  it.each([
    ["有效", false, 7],
    ["缺失 TDD 证据", true, 8],
  ])(
    "partial retry 对%s DONE result 的执行数正确",
    async (_name, tamper, retryCalls) => {
      let failing = true;
      const execute = vi.fn(async ({ task }) => ({
        modifiedFiles: [],
        ...(failing && task.phase === "GREEN"
          ? {
              tddEvidence: [
                {
                  phase: "GREEN",
                  command: "npm test",
                  passed: false,
                  output: "failed",
                },
              ],
              verification: [],
            }
          : evidenceFor(task.phase)),
      }));
      const { root, core } = await plannedProject({ execute });
      expect(await core.execute({ command: "build", cwd: root })).toMatchObject(
        { ok: false },
      );
      if (tamper) {
        const path = join(root, ".sdd/changes/add-cancel/task-results.json");
        const results = JSON.parse(await readFile(path, "utf8"));
        delete results[0].tddEvidence;
        await writeFile(path, `${JSON.stringify(results, null, 2)}\n`, "utf8");
      }
      execute.mockClear();
      failing = false;
      expect(await core.execute({ command: "build", cwd: root })).toMatchObject(
        { ok: true },
      );
      expect(execute).toHaveBeenCalledTimes(retryCalls);
    },
  );

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

  it("恢复 build 时若状态上下文与命令不匹配则返回 E_STATE_CORRUPTED", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-build-"));
    roots.push(root);
    await writeFile(join(root, "README.md"), "# Orders\n", "utf8");
    await writeFile(join(root, ".sdd/state.json"), "{}\n", "utf8").catch(
      () => undefined,
    );
    const store = new StateStore(root);
    await store.write({
      ...createInitialState(),
      initialized: true,
      currentChangeId: "add-cancel",
      currentRunId: "run-1",
      currentPhase: "FAILED",
      previousPhase: "SPEC_READY",
      inProgressPhase: "DESIGNING",
      failedCommand: "sdd build",
      suggestedCommand: "sdd build",
      tasks: { "TASK-001": "DONE" },
    });
    const core = new Core({
      codebase: new CodebaseAdapter(),
      taskExecutor: { execute: vi.fn() },
    });

    const result = await core.execute({ command: "build", cwd: root });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      error: { code: "E_STATE_CORRUPTED" },
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
      const area = task.id.startsWith("TASK-001") ? "orders" : "audit";
      return {
        modifiedFiles: [`src/${area}/index.ts`],
        verification: [{ command: "npm test", passed: true, output: "passed" }],
      };
    });
    const { root, core } = await plannedProject({ execute });
    const change = join(root, ".sdd/changes/add-cancel");
    const phases = ["RED", "GREEN", "REFACTOR", "VERIFY"] as const;
    const tasks = [
      ...phases.map((phase, index) => ({
        id: `TASK-001-${phase}`,
        title: "Orders",
        phase,
        status: "PENDING" as const,
        requirements: ["REQ-002"],
        scenarios: ["SCN-001"],
        dependsOn: index === 0 ? [] : [`TASK-001-${phases[index - 1]}`],
        allowedFiles: ["src/orders/**"],
        expectedNewFiles: ["src/orders/**"],
        forbiddenFiles: [".git/**"],
        verification: ["npm test"],
        doneCriteria: ["done"],
      })),
      ...phases.map((phase, index) => ({
        id: `TASK-002-${phase}`,
        title: "Audit",
        phase,
        status: "PENDING" as const,
        requirements: ["REQ-001"],
        scenarios: ["SCN-002"],
        dependsOn: index === 0 ? [] : [`TASK-002-${phases[index - 1]}`],
        allowedFiles: ["src/audit/**"],
        expectedNewFiles: ["src/audit/**"],
        forbiddenFiles: [".git/**"],
        verification: ["npm test"],
        doneCriteria: ["done"],
      })),
    ];
    await writeFile(
      join(change, "tasks.json"),
      `${JSON.stringify(tasks, null, 2)}\n`,
      "utf8",
    );
    const [spec, design, impact, codebaseSummary] = await Promise.all([
      readFile(join(change, "spec.md"), "utf8"),
      readFile(join(change, "design.md"), "utf8"),
      readFile(join(change, "impact.md"), "utf8"),
      readFile(join(root, ".sdd/index/codebase-summary.md"), "utf8"),
    ]);
    const tasksMarkdown = [
      "# Tasks",
      "",
      ...tasks.map((task) => `## ${task.id} ${task.title}\n`),
    ].join("\n");
    await writeFile(join(change, "tasks.md"), tasksMarkdown, "utf8");
    const codebaseIndexHash = artifactInputHash(codebaseSummary);
    const sourceArtifactHash = artifactInputHash({
      spec,
      design,
      impact,
      tasksMarkdown,
      tasksJson: JSON.stringify(tasks, null, 2),
    });
    const contextPack = [
      "<!-- Context Pack Metadata",
      `Codebase Index Hash: ${codebaseIndexHash}`,
      `Source Artifact Hash: ${sourceArtifactHash}`,
      "Generated At: 2026-07-04T00:00:00.000Z",
      "-->",
      "",
      "# TASK",
      "",
      "Allowed Files",
      "",
      "Risk",
      "",
    ].join("\n");
    await Promise.all(
      tasks.map((task) =>
        writeFile(
          join(root, ".sdd/context-packs/add-cancel", `${task.id}.md`),
          contextPack.replace("# TASK", `# ${task.id}`),
          "utf8",
        ),
      ),
    );
    const statePath = join(root, ".sdd/state.json");
    const state = JSON.parse(await readFile(statePath, "utf8"));
    state.tasks = Object.fromEntries(tasks.map((task) => [task.id, "PENDING"]));
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
