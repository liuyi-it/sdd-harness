import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import { artifactInputHash } from "../src/artifacts/artifact-writer.js";
import { renderContextPack } from "../src/build/context-pack.js";
import { normalizeTaskExecutionResult } from "../src/build/task-result-normalizer.js";
import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";
import { type TaskExecutor } from "../src/build/task-executor.js";
import { GitIsolationManager } from "../src/git-isolation/manager.js";
import { resolveProjectRules } from "../src/project-conventions/rule-resolver.js";
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

function projectRulesHash(content: string): string | undefined {
  return content.match(/^Project Rules Hash: (sha256:[a-f0-9]{64})$/m)?.[1];
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
        if (typeof result !== "object" || result === null) return result;
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
  it("手动 build next 持久化 handoff，重复调用返回同一任务，并接受标准 V2 制品", async () => {
    const { core, root } = await plannedProject({ execute: vi.fn() });
    const first = await core.execute({
      command: "build",
      cwd: root,
      args: { subcommand: "next" },
    });
    const repeated = await core.execute({
      command: "build",
      cwd: root,
      args: { subcommand: "next" },
    });
    expect(first.actionRequired?.type).toBe("AGENT_TASK_EXECUTION");
    expect(repeated.actionRequired).toMatchObject({
      taskId: first.actionRequired?.taskId,
      resultFile: first.actionRequired?.resultFile,
    });
    const taskId = first.actionRequired!.taskId;
    const minimality = {
      reusedExisting: ["src/order.ts"],
      standardLibraryChoices: [],
      nativePlatformChoices: [],
      dependenciesAdded: [],
      abstractionsAdded: [],
      deliberateDebts: [],
    };
    const artifact = normalizeTaskExecutionResult(
      {
        taskId,
        modifiedFiles: [],
        ...evidenceFor("RED"),
        minimality,
      } as Parameters<typeof normalizeTaskExecutionResult>[0],
      {
        actualFileDelta: { added: [], modified: [], deleted: [] },
        startedAt: new Date().toISOString(),
        endedAt: new Date().toISOString(),
        requestedMode: "main-agent",
        actualMode: "main-agent",
      },
    );
    artifact.taskId = taskId;
    expect(
      await core.execute({
        command: "build",
        cwd: root,
        args: { subcommand: "complete", taskId, result: artifact },
      }),
    ).toMatchObject({ ok: true, state: "PLAN_READY" });
    const persisted = JSON.parse(
      await readFile(join(root, first.actionRequired!.resultFile), "utf8"),
    );
    expect(persisted).toMatchObject({
      schemaVersion: "1.2.0",
      taskId,
      summary: artifact.summary,
      commandEvidence: artifact.commandEvidence,
      timestamps: artifact.timestamps,
      mode: artifact.mode,
      fileDelta: { added: [], modified: [], deleted: [] },
      minimality,
      legacy: { modifiedFiles: [] },
    });
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/changes/add-cancel/task-results.json"),
          "utf8",
        ),
      )[0],
    ).toMatchObject({ taskId, minimality });
  });

  it("外部 Agent 隐瞒越权文件修改时按真实 Git delta 阻断", async () => {
    const { core, root } = await plannedProject({ execute: vi.fn() });
    const handoff = await core.execute({
      command: "build",
      cwd: root,
      args: { subcommand: "next" },
    });
    const taskId = handoff.actionRequired!.taskId;
    await writeFile(join(root, ".env"), "SECRET=hidden\n", "utf8");

    const result = await core.execute({
      command: "build",
      cwd: root,
      args: {
        subcommand: "complete",
        taskId,
        result: {
          schemaVersion: "1.0.0",
          taskId,
          status: "SUCCEEDED",
          modifiedFiles: [],
          ...evidenceFor("RED"),
        },
      },
    });

    expect(result).toMatchObject({
      ok: false,
      exitCode: 10,
      error: {
        code: "E_UNDECLARED_FILE_CHANGE",
        message: expect.stringContaining(".env"),
      },
    });
  });

  it("Agent 返回非成功状态时终止 handoff 而不是把失败当作成功", async () => {
    const { core, root } = await plannedProject({ execute: vi.fn() });
    const handoff = await core.execute({
      command: "build",
      cwd: root,
      args: { subcommand: "next" },
    });
    const taskId = handoff.actionRequired!.taskId;
    expect(
      await core.execute({
        command: "build",
        cwd: root,
        args: {
          subcommand: "complete",
          taskId,
          result: {
            schemaVersion: "1.0.0",
            taskId,
            status: "FAILED",
            modifiedFiles: [],
            ...evidenceFor("RED"),
          },
        },
      }),
    ).toMatchObject({
      ok: false,
      error: { code: "E_AGENT_TASK_FAILED" },
    });
    expect((await new StateStore(root).read()).tasks[taskId]).toBe("FAILED");
  });

  it("损坏的 task-results 元素返回结构化状态错误", async () => {
    const { core, root } = await plannedProject({ execute: vi.fn() });
    await writeFile(
      join(root, ".sdd/changes/add-cancel/task-results.json"),
      "[null]",
    );
    await expect(
      core.execute({ command: "build", cwd: root }),
    ).resolves.toMatchObject({
      ok: false,
      error: { code: "E_STATE_CORRUPTED" },
    });
  });

  it("缺少 TDD 证据时拒绝完成任务", async () => {
    const { core, root } = await plannedProject({
      execute: vi.fn().mockResolvedValue({
        modifiedFiles: [],
        tddEvidence: [],
        verification: [{ command: "npm test", passed: true, output: "pass" }],
      }),
    });

    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: false,
      exitCode: 7,
      error: {
        code: "E_TDD_EVIDENCE_REQUIRED",
        message: expect.stringContaining("缺少 RED 阶段证据"),
      },
    });
  });

  it("RED 同时记录预期失败和辅助通过证据时允许完整链完成", async () => {
    const { core, root } = await plannedProject({
      execute: vi.fn(async ({ task }) => ({
        modifiedFiles: [],
        ...(task.phase === "RED"
          ? {
              tddEvidence: [
                {
                  phase: "RED",
                  command: "npm test",
                  passed: false,
                  expectedFailure: true,
                  output: "target failed",
                },
                {
                  phase: "RED",
                  command: "npm test",
                  passed: true,
                  output: "helper passed",
                },
              ],
              verification: [],
            }
          : evidenceFor(task.phase)),
      })),
    });

    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: true,
      state: "BUILD_READY",
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

  it("v2 结果的嵌套数组元素畸形时返回稳定错误", async () => {
    const { core, root } = await plannedProject({
      execute: vi.fn().mockResolvedValue({
        schemaVersion: "1.2.0",
        status: "SUCCEEDED",
        summary: "done",
        commandEvidence: [
          {
            command: "npm",
            args: [1],
            outputSummary: "invalid args",
          },
        ],
        fileDelta: { added: [], modified: [], deleted: [] },
        timestamps: {
          startedAt: "2026-07-13T00:00:00.000Z",
          endedAt: "2026-07-13T00:00:01.000Z",
        },
        legacy: {
          modifiedFiles: [],
          tddEvidence: [],
          verification: [],
        },
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

  it("refreshes context packs when project rules change after plan", async () => {
    const { core, root } = await plannedProject({
      execute: vi.fn(async ({ task }) => ({
        modifiedFiles: ["src/order.ts", "test/order.test.ts"],
        ...evidenceFor(task.phase),
      })),
    });
    const packPath = join(
      root,
      ".sdd/context-packs/add-cancel/TASK-001-RED.md",
    );
    await core.execute({
      command: "build",
      cwd: root,
      args: { subcommand: "next" },
    });
    const before = await readFile(packPath, "utf8");
    await writeFile(
      join(root, "AGENTS.md"),
      `${await readFile(join(root, "AGENTS.md"), "utf8")}\n## Extra rule\n`,
      "utf8",
    );

    const result = await core.execute({
      command: "build",
      cwd: root,
      args: { subcommand: "next" },
    });

    expect(result).toMatchObject({
      ok: true,
      state: "BUILD_WAITING_AGENT",
    });
    const after = await readFile(packPath, "utf8");
    expect(projectRulesHash(after)).toMatch(/^sha256:[a-f0-9]{64}$/);
    expect(projectRulesHash(after)).not.toBe(projectRulesHash(before));
  });

  it("Context Pack 的 allowedFiles 被篡改后从权威任务定义重建", async () => {
    const { core, root } = await plannedProject({
      execute: vi.fn(async ({ task }) => ({
        modifiedFiles: [],
        ...evidenceFor(task.phase),
      })),
    });
    const packPath = join(
      root,
      ".sdd/context-packs/add-cancel/TASK-001-RED.md",
    );
    await core.execute({
      command: "build",
      cwd: root,
      args: { subcommand: "next" },
    });
    const original = await readFile(packPath, "utf8");
    await writeFile(
      packPath,
      original.replaceAll("- src/order.ts", "- **"),
      "utf8",
    );

    expect(
      await core.execute({
        command: "build",
        cwd: root,
        args: { subcommand: "next" },
      }),
    ).toMatchObject({
      ok: true,
      state: "BUILD_WAITING_AGENT",
    });
    const repaired = await readFile(packPath, "utf8");
    expect(repaired).toContain("- src/order.ts");
    expect(
      repaired.match(
        /### Allowed Files\n\n([\s\S]*?)\n\n### Forbidden Files/u,
      )?.[1],
    ).not.toContain("- **");
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

  it.each([
    null,
    {},
    { modifiedFiles: "src/order.ts", tddEvidence: [], verification: [] },
  ])("executor 畸形返回值 %# 稳定进入 FAILED", async (executorResult) => {
    const { core, root } = await plannedProject({
      execute: vi.fn().mockResolvedValue(executorResult),
    });
    const result = await core.execute({ command: "build", cwd: root });
    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      error: { code: "E_TDD_EVIDENCE_REQUIRED" },
    });
    expect(
      JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
    ).toMatchObject({ currentPhase: "FAILED", failedCommand: "sdd build" });
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

  it("运行级任务制品使用 Git delta 裁决最终 fileDelta", async () => {
    const execute = vi.fn(async ({ root, task }) => {
      const target =
        task.phase === "RED"
          ? join(root, "src/order.ts")
          : join(root, "test/order.test.ts");
      await writeFile(target, `// ${task.id}\n`, "utf8");
      return {
        modifiedFiles: ["declared-but-unused.ts"],
        ...evidenceFor(task.phase),
      };
    });
    const { core, root } = await plannedProject({ execute });

    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: true,
      state: "BUILD_READY",
    });

    const runId = JSON.parse(
      await readFile(join(root, ".sdd/state.json"), "utf8"),
    ).currentRunId;
    const persisted = JSON.parse(
      await readFile(
        join(root, ".sdd/runs", runId, "tasks", "TASK-001-RED.result.json"),
        "utf8",
      ),
    );

    expect(persisted.fileDelta).toMatchObject({
      added: [],
      modified: ["src/order.ts"],
      deleted: [],
    });
  });

  it("在隔离工作区执行任务，但 .sdd 与主工作区状态仍写回 controlRoot", async () => {
    const seenRoots: string[] = [];
    const execute = vi.fn(async ({ root, task }) => {
      seenRoots.push(root);
      const target =
        task.phase === "RED"
          ? join(root, "src/order.ts")
          : join(root, "test/order.test.ts");
      await writeFile(target, `// isolated ${task.id}\n`, "utf8");
      return {
        modifiedFiles: [
          target.endsWith("order.ts") ? "src/order.ts" : "test/order.test.ts",
        ],
        ...evidenceFor(task.phase),
      };
    });
    const { core, root } = await plannedProject({ execute });
    const manager = new GitIsolationManager(root, {
      createBranch: true,
      createWorktree: true,
      branchPattern: "sdd/<change-id>",
      worktreeDir: ".sdd/worktrees",
    });
    const workspace = await manager.ensure("add-cancel");
    const store = new StateStore(root);
    await store.write({
      ...(await store.read()),
      workspace: {
        branchName: workspace.branchName,
        worktreePath: workspace.worktreePath,
        baselineCommit: workspace.baselineCommit,
      },
    });

    const result = await core.execute({ command: "build", cwd: root });

    expect(result).toMatchObject({ ok: true, state: "BUILD_READY" });
    expect(seenRoots.length).toBeGreaterThan(0);
    expect(seenRoots.every((entry) => entry === workspace.businessRoot)).toBe(
      true,
    );
    expect(await readFile(join(root, "src/order.ts"), "utf8")).toBe(
      "export const order = {};\n",
    );
    expect(
      await readFile(join(workspace.businessRoot, "src/order.ts"), "utf8"),
    ).toContain("isolated");
    const state = JSON.parse(
      await readFile(join(root, ".sdd/state.json"), "utf8"),
    ) as {
      currentPhase: string;
      workspace?: {
        branchName: string | null;
        worktreePath: string | null;
        baselineCommit: string;
      };
    };
    expect(state.currentPhase).toBe("BUILD_READY");
    expect(state.workspace).toMatchObject({
      branchName: "sdd/add-cancel",
      baselineCommit: workspace.baselineCommit,
    });
    const runId = state.currentRunId as string;
    await expect(
      readFile(
        join(root, ".sdd/runs", runId, "tasks", "TASK-001-RED.result.json"),
        "utf8",
      ),
    ).resolves.toContain('"taskId": "TASK-001-RED"');
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
      execute: vi.fn(async ({ root }) => {
        await writeFile(join(root, "secrets.txt"), "secret\n", "utf8");
        return {
          modifiedFiles: ["secrets.txt"],
          verification: [{ command: "npm test", passed: true, output: "pass" }],
        };
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

  it("当 spec/design/tasks 变化导致 Context Pack 失效时自动刷新后继续执行", async () => {
    const execute = vi.fn().mockResolvedValue({
      modifiedFiles: ["src/order.ts"],
      verification: [{ command: "npm test", passed: true, output: "passed" }],
    });
    const { root, core } = await plannedProject({ execute });
    const packPath = join(
      root,
      ".sdd/context-packs/add-cancel/TASK-001-RED.md",
    );
    await core.execute({
      command: "build",
      cwd: root,
      args: { subcommand: "next" },
    });
    const before = await readFile(packPath, "utf8");
    await writeFile(
      join(root, ".sdd/changes/add-cancel/design.md"),
      "# 用户修改后的设计\n",
      "utf8",
    );

    const result = await core.execute({
      command: "build",
      cwd: root,
      args: { subcommand: "next" },
    });

    expect(result).toMatchObject({
      ok: true,
      state: "BUILD_WAITING_AGENT",
    });
    const refreshed = await readFile(packPath, "utf8");
    expect(refreshed).not.toBe(before);
    expect(projectRulesHash(refreshed)).toBe(projectRulesHash(before));
  });

  it("当代码库索引变化导致 Context Pack 失效时自动刷新后继续执行", async () => {
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
      ok: true,
      state: "BUILD_READY",
    });
    expect(execute).toHaveBeenCalled();
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
    let failAudit = true;
    const execute = vi.fn(
      async ({ task }: { task: { id: string; phase: string } }) => {
        active += 1;
        peak = Math.max(peak, active);
        await new Promise((resolve) => setTimeout(resolve, 20));
        active -= 1;
        const area = task.id.startsWith("TASK-001") ? "orders" : "audit";
        return {
          modifiedFiles: [`src/${area}/index.ts`],
          ...(failAudit && task.id === "TASK-002-RED"
            ? { tddEvidence: [] }
            : {}),
          verification: [
            { command: "npm test", passed: true, output: "passed" },
          ],
        };
      },
    );
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
        scenarios: ["REQ-002-SC-001"],
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
        scenarios: ["REQ-001-SC-001"],
        dependsOn: index === 0 ? [] : [`TASK-002-${phases[index - 1]}`],
        allowedFiles: ["src/audit/**"],
        expectedNewFiles: ["src/audit/**"],
        forbiddenFiles: [".git/**"],
        verification: ["npm test"],
        doneCriteria: ["done"],
      })),
    ];
    const [spec, design, compactSpec, codebaseSummary] = await Promise.all([
      readFile(join(change, "spec.md"), "utf8"),
      readFile(join(change, "design.md"), "utf8"),
      readFile(join(change, "spec.json"), "utf8").then(JSON.parse),
      readFile(join(root, ".sdd/index/codebase-summary.md"), "utf8"),
    ]);
    const impact = compactSpec.impact as string;
    const tasksMarkdown = [
      "# Tasks",
      "",
      ...tasks.map((task) => `## ${task.id} ${task.title}\n`),
    ].join("\n");
    await writeFile(
      join(change, "plan.json"),
      `${JSON.stringify(
        {
          schemaVersion: "2.0.0",
          tasks,
          tasksMarkdown,
          testPlan: "# Test Plan",
          context: "# Context",
        },
        null,
        2,
      )}\n`,
      "utf8",
    );
    const projectConventionsHash = artifactInputHash(
      await readFile(join(root, ".sdd/project/conventions.json"), "utf8"),
    );
    const body = ["# TASK", "", "Allowed Files", "", "Risk", "", ""].join("\n");
    await mkdir(join(root, ".sdd/context-packs/add-cancel"), {
      recursive: true,
    });
    await Promise.all(
      tasks.map(async (task) =>
        writeFile(
          join(root, ".sdd/context-packs/add-cancel", `${task.id}.md`),
          renderContextPack({
            body: body.replace("# TASK", `# ${task.id}`),
            rules: await resolveProjectRules(
              root,
              [...task.allowedFiles, ...task.expectedNewFiles],
              "codex",
            ),
            codebaseSummary,
            spec,
            design,
            impact,
            tasksMarkdown,
            tasksJson: JSON.stringify(tasks, null, 2),
            projectConventionsHash,
            references: {
              spec: ".sdd/changes/add-cancel/spec.md",
              design: ".sdd/changes/add-cancel/design.md",
              plan: ".sdd/changes/add-cancel/plan.json",
              impact: ".sdd/changes/add-cancel/spec.json",
              codebase: ".sdd/index/codebase-summary.md",
            },
            task: {
              taskId: task.id,
              objective: task.title,
              userVisibleOutcome: task.userVisibleOutcome ?? task.title,
              requiredFiles: task.allowedFiles,
              allowedFiles: task.allowedFiles,
              forbiddenFiles: task.forbiddenFiles,
              verification: task.verification,
            },
          }),
          "utf8",
        ),
      ),
    );
    const statePath = join(root, ".sdd/state.json");
    const state = JSON.parse(await readFile(statePath, "utf8"));
    state.tasks = Object.fromEntries(tasks.map((task) => [task.id, "PENDING"]));
    await writeFile(statePath, `${JSON.stringify(state)}\n`, "utf8");

    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: false,
      state: "FAILED",
    });
    expect(peak).toBe(2);
    let persisted = JSON.parse(
      await readFile(join(change, "task-results.json"), "utf8"),
    );
    expect(
      persisted.map((entry: { taskId: string }) => entry.taskId),
    ).toContain("TASK-001-RED");
    expect(JSON.parse(await readFile(statePath, "utf8")).tasks).toMatchObject({
      "TASK-001-RED": "DONE",
      "TASK-002-RED": "FAILED",
    });
    execute.mockClear();
    failAudit = false;
    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: true,
      state: "BUILD_READY",
    });
    expect(
      execute.mock.calls.map(([request]) => request.task.id),
    ).not.toContain("TASK-001-RED");
    const runId = JSON.parse(await readFile(statePath, "utf8")).currentRunId;
    for (const task of tasks) {
      await expect(
        readFile(
          join(root, ".sdd/runs", runId, "tasks", `${task.id}.result.json`),
          "utf8",
        ),
      ).resolves.toContain(task.id);
    }
    persisted = JSON.parse(
      await readFile(join(change, "task-results.json"), "utf8"),
    );
    expect(persisted).toHaveLength(8);
  });
});
