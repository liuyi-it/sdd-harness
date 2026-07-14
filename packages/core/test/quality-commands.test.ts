import { execFileSync } from "node:child_process";
import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import { ArtifactWriter } from "../src/artifacts/artifact-writer.js";
import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";
import { prepareRepairTasks } from "../src/commands/repair-task.js";
import { GitInspector } from "../src/git/git-inspector.js";
import { StateStore } from "../src/state/state-store.js";
import type { StoredTaskResult } from "../src/quality/quality-gates.js";
import { traceabilityFailures } from "../src/quality/traceability.js";
import type { SpecDocument } from "../src/engines/openspec/model.js";
import type {
  TaskDefinition,
  TddPhase,
} from "../src/engines/tdd/tdd-engine.js";

type RmFunction = typeof rm;

// 这组测试把 verify/review/archive 串起来，验证“完成证据”最终能沉淀成可追踪归档。
const roots: string[] = [];

async function readPlan(root: string) {
  return JSON.parse(
    await readFile(join(root, ".sdd/changes/add-cancel/plan.json"), "utf8"),
  ) as { tasks: Array<Record<string, unknown>>; [key: string]: unknown };
}

async function writePlan(root: string, plan: Record<string, unknown>) {
  await writeFile(
    join(root, ".sdd/changes/add-cancel/plan.json"),
    `${JSON.stringify(plan, null, 2)}\n`,
    "utf8",
  );
}

async function builtProject(): Promise<{ root: string; core: Core }> {
  const root = await mkdtemp(join(tmpdir(), "sdd-quality-"));
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
  await core.execute({ command: "build", cwd: root });
  return { root, core };
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await new Promise((resolve) => setTimeout(resolve, 150));
  await Promise.all(roots.splice(0).map((root) => removeRetry(root, rm)));
  vi.restoreAllMocks();
});

async function removeRetry(root: string, rm: RmFunction): Promise<void> {
  for (let attempt = 0; attempt < 5; attempt += 1) {
    try {
      await rm(root, { recursive: true, force: true });
      return;
    } catch (error) {
      if (
        !(error instanceof Error) ||
        !("code" in error) ||
        error.code !== "ENOTEMPTY" ||
        attempt === 4
      ) {
        throw error;
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
}

describe("quality commands", () => {
  it("逐 Scenario 检查模型覆盖、幽灵 ID 与四阶段证据", () => {
    const fixture = traceFixture();
    fixture.document.requirements[0]!.scenarios.push({
      id: "REQ-001-SC-002",
      title: "未覆盖场景",
      given: ["条件"],
      when: ["操作"],
      then: ["结果"],
    });
    fixture.tasks[0]!.requirements = ["REQ-GHOST"];

    const failures = traceabilityFailures(
      fixture.document,
      fixture.tasks,
      fixture.results,
    );

    expect(failures).toContain("TASK-RED 引用了不存在的 Requirement REQ-GHOST");
    expect(failures).toContain("REQ-001/REQ-001-SC-002 缺少 RED 任务");
    expect(failures).toContain("REQ-001/REQ-001-SC-002 缺少 VERIFY 任务");
  });

  it.each<TddPhase>(["RED", "GREEN", "REFACTOR", "VERIFY"])(
    "缺少 %s 阶段任务时追踪失败",
    (phase) => {
      const fixture = traceFixture();
      fixture.tasks = fixture.tasks.filter((task) => task.phase !== phase);
      expect(
        traceabilityFailures(fixture.document, fixture.tasks, fixture.results),
      ).toContain(`REQ-001/REQ-001-SC-001 缺少 ${phase} 任务`);
    },
  );

  it.each<TddPhase>(["RED", "GREEN", "REFACTOR", "VERIFY"])(
    "缺少 %s 命令时追踪失败",
    (phase) => {
      const fixture = traceFixture();
      const result = fixture.results.find(
        (entry) => entry.taskId === `TASK-${phase}`,
      )!;
      result.tddEvidence = [];
      expect(
        traceabilityFailures(fixture.document, fixture.tasks, fixture.results),
      ).toContain(`TASK-${phase} 缺少 ${phase} 命令`);
    },
  );

  it("完整多 Scenario 四阶段追踪通过", () => {
    const fixture = traceFixture();
    fixture.document.requirements[0]!.scenarios.push({
      id: "REQ-001-SC-002",
      title: "第二场景",
      given: ["条件"],
      when: ["操作"],
      then: ["结果"],
    });
    fixture.tasks.forEach((task) => task.scenarios.push("REQ-001-SC-002"));

    expect(
      traceabilityFailures(fixture.document, fixture.tasks, fixture.results),
    ).toEqual([]);
  });

  it("verifies requirement, task, acceptance, and test coverage", async () => {
    const { root, core } = await builtProject();
    const result = await core.execute({
      command: "verify",
      cwd: root,
      args: { changeId: "add-cancel" },
    });
    expect(result).toMatchObject({
      ok: true,
      state: "VERIFY_READY",
      next: "sdd review",
    });
    expect(
      await readFile(
        join(root, ".sdd/changes/add-cancel/verify-report.md"),
        "utf8",
      ),
    ).toContain("## Result\n\nPASS");
    expect(
      await new ArtifactWriter().metadata(
        join(root, ".sdd/changes/add-cancel/verify-report.md"),
      ),
    ).toMatchObject({
      schemaVersion: "1.0.0",
      generatedBy: "sdd-harness",
      inputHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      artifactHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      createdAt: expect.any(String),
    });
  }, 15_000);

  it("当 Git 未变化时重复执行 verify 直接复用上次结果", async () => {
    const { root, core } = await builtProject();

    expect(await core.execute({ command: "verify", cwd: root })).toMatchObject({
      ok: true,
      state: "VERIFY_READY",
      next: "sdd review",
    });

    const repeated = await core.execute({ command: "verify", cwd: root });

    expect(repeated).toMatchObject({
      ok: true,
      state: "VERIFY_READY",
      next: "sdd review",
      data: { alreadyReady: true },
    });
  });

  it("当 --change 与当前活动变更不一致时拒绝执行 verify", async () => {
    const { root, core } = await builtProject();

    const result = await core.execute({
      command: "verify",
      cwd: root,
      args: { changeId: "other-change" },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "BUILD_READY",
      error: { code: "E_MISSING_CHANGE" },
    });
  });

  it("verify 修复需要扩大文件范围时暂停并请求用户决策", async () => {
    const { root, core } = await builtProject();
    await writeFile(join(root, "notes.txt"), "manual drift\n", "utf8");

    const result = await core.execute({ command: "verify", cwd: root });

    expect(result).toMatchObject({
      ok: false,
      state: "PAUSED",
      error: { code: "E_VERIFY_FAILED", next: "sdd status" },
    });
    expect(
      await readFile(
        join(root, ".sdd/changes/add-cancel/verify-report.md"),
        "utf8",
      ),
    ).toContain("未跟踪到任务结果的变更文件：notes.txt");
    const tasks = (await readPlan(root)).tasks as Array<{
      sliceType?: string;
      failureContext?: { source: string; errorCode: string };
      policyRefs?: Array<{ id: string }>;
    }>;
    expect(tasks.filter((task) => task.sliceType === "REPAIR")).toHaveLength(0);
  });

  it("verify 会复核持久化的 TDD 证据", async () => {
    const { root, core } = await builtProject();
    const resultPath = join(root, ".sdd/changes/add-cancel/task-results.json");
    const results = JSON.parse(await readFile(resultPath, "utf8"));
    results[0].tddEvidence = [];
    await writeFile(
      resultPath,
      `${JSON.stringify(results, null, 2)}\n`,
      "utf8",
    );

    const result = await core.execute({ command: "verify", cwd: root });

    expect(result).toMatchObject({
      ok: false,
      state: "PLAN_READY",
      error: { code: "E_VERIFY_FAILED", next: "sdd build next" },
    });
    const tasks = (await readPlan(root)).tasks as Array<{
      sliceType?: string;
      failureContext?: { source: string; errorCode: string };
      policyRefs?: Array<{ id: string }>;
    }>;
    const repairs = tasks.filter((task) => task.sliceType === "REPAIR");
    expect(repairs).toHaveLength(4);
    expect(repairs[0]?.failureContext).toMatchObject({
      source: "VERIFY",
      errorCode: "E_VERIFY_FAILED",
    });
    expect(repairs[0]?.policyRefs?.map(({ id }) => id)).toContain(
      "systematic-diagnosis",
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
      actionRequired: {
        policyBundle: {
          policies: expect.arrayContaining([
            expect.objectContaining({ id: "systematic-diagnosis" }),
          ]),
        },
      },
    });
  });

  it("同一失败签名达到预算后暂停，不产生无限 REPAIR", async () => {
    const { root, core } = await builtProject();
    const resultPath = join(root, ".sdd/changes/add-cancel/task-results.json");
    const results = JSON.parse(await readFile(resultPath, "utf8"));
    results[0].tddEvidence = [];
    await writeFile(
      resultPath,
      `${JSON.stringify(results, null, 2)}\n`,
      "utf8",
    );
    const first = await core.execute({ command: "verify", cwd: root });
    const message = first.error?.message ?? "verify failed";

    const second = await prepareRepairTasks(root, "add-cancel", {
      source: "VERIFY",
      errorCode: "E_VERIFY_FAILED",
      message,
    });
    const exhausted = await prepareRepairTasks(root, "add-cancel", {
      source: "VERIFY",
      errorCode: "E_VERIFY_FAILED",
      message,
    });

    expect(second).toMatchObject({ created: true, paused: false });
    expect(exhausted).toEqual({ created: false, paused: true, taskIds: [] });
    expect(
      JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
    ).toMatchObject({
      currentPhase: "PAUSED",
      suggestedCommand: "sdd status",
    });
    const tasks = (await readPlan(root)).tasks as Array<{ sliceType?: string }>;
    expect(tasks.filter((task) => task.sliceType === "REPAIR")).toHaveLength(8);
  });

  it("VERIFY_READY 重复 verify 也会重新复核被篡改的 TDD 证据", async () => {
    const { root, core } = await builtProject();
    expect(await core.execute({ command: "verify", cwd: root })).toMatchObject({
      ok: true,
    });
    const resultPath = join(root, ".sdd/changes/add-cancel/task-results.json");
    const results = JSON.parse(await readFile(resultPath, "utf8"));
    results[0].tddEvidence = [];
    await writeFile(
      resultPath,
      `${JSON.stringify(results, null, 2)}\n`,
      "utf8",
    );

    expect(await core.execute({ command: "verify", cwd: root })).toMatchObject({
      ok: false,
      error: { code: "E_VERIFY_FAILED" },
    });
  });

  it("spec.json 模型损坏时 verify 不回退到 Markdown", async () => {
    const { root, core } = await builtProject();
    const path = join(root, ".sdd/changes/add-cancel/spec.json");
    const compact = JSON.parse(await readFile(path, "utf8"));
    compact.model = { title: "损坏", requirements: "不是数组" };
    await writeFile(path, `${JSON.stringify(compact, null, 2)}\n`, "utf8");

    expect(await core.execute({ command: "verify", cwd: root })).toMatchObject({
      ok: false,
      error: { code: "E_STATE_CORRUPTED" },
    });
  });

  it("verify 拒绝任务引用不存在的 Scenario", async () => {
    const { root, core } = await builtProject();
    const plan = await readPlan(root);
    plan.tasks[0]!.scenarios = ["REQ-999-SC-001"];
    await writePlan(root, plan);

    const result = await core.execute({ command: "verify", cwd: root });

    expect(result).toMatchObject({
      ok: false,
      error: { code: "E_VERIFY_FAILED" },
    });
    expect(result.error?.message).toContain("REQ-999-SC-001");
  });

  it("缺少 spec.json 时拒绝读取旧结构", async () => {
    const { root, core } = await builtProject();
    const change = join(root, ".sdd/changes/add-cancel");
    await rm(join(change, "spec.json"));

    const result = await core.execute({ command: "verify", cwd: root });
    expect(result).toMatchObject({
      ok: false,
      error: { code: "E_STATE_CORRUPTED" },
    });
  });

  it("verify 在超时后进入 FAILED", async () => {
    const { root, core } = await builtProject();
    const originalSnapshot = GitInspector.prototype.snapshot;
    vi.spyOn(GitInspector.prototype, "snapshot").mockImplementation(
      async function (this: GitInspector) {
        await new Promise((resolve) => setTimeout(resolve, 50));
        return await originalSnapshot.call(this);
      },
    );

    const result = await core.execute({
      command: "verify",
      cwd: root,
      args: { timeout: 0.01 },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      exitCode: 124,
      error: { code: "E_TIMEOUT", next: "sdd verify" },
    });
  });

  it("reviews scope and implementation evidence", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    const result = await core.execute({
      command: "review",
      cwd: root,
      args: { changeId: "add-cancel" },
    });
    expect(result).toMatchObject({
      ok: true,
      state: "REVIEW_READY",
      next: "sdd archive",
    });
    expect(
      await readFile(
        join(root, ".sdd/changes/add-cancel/review-report.md"),
        "utf8",
      ),
    ).toContain("## Result\n\nPASS");
    expect(
      await new ArtifactWriter().metadata(
        join(root, ".sdd/changes/add-cancel/review-report.md"),
      ),
    ).toMatchObject({
      schemaVersion: "1.0.0",
      generatedBy: "sdd-harness",
      inputHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      artifactHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      createdAt: expect.any(String),
    });
  });

  it("当 Git 未变化时重复执行 review 直接复用上次结果", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });

    expect(await core.execute({ command: "review", cwd: root })).toMatchObject({
      ok: true,
      state: "REVIEW_READY",
      next: "sdd archive",
    });

    const repeated = await core.execute({ command: "review", cwd: root });

    expect(repeated).toMatchObject({
      ok: true,
      state: "REVIEW_READY",
      next: "sdd archive",
      data: { alreadyReady: true },
    });
  });

  it("当 --change 与当前活动变更不一致时拒绝执行 review", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });

    const result = await core.execute({
      command: "review",
      cwd: root,
      args: { changeId: "other-change" },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "VERIFY_READY",
      error: { code: "E_MISSING_CHANGE" },
    });
  });

  it("review 失败生成 REPAIR 任务并阻止归档", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    await writeFile(
      join(root, "src/order.ts"),
      "export const leaked = 'ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789';\n",
      "utf8",
    );

    const result = await core.execute({ command: "review", cwd: root });

    expect(result).toMatchObject({
      ok: false,
      state: "PLAN_READY",
      error: { code: "E_REVIEW_FAILED", next: "sdd build next" },
    });
    expect(
      await readFile(
        join(root, ".sdd/changes/add-cancel/review-report.v2.md"),
        "utf8",
      ),
    ).toContain("SECRET_LEAK");
    const tasks = (await readPlan(root)).tasks as Array<{
      sliceType?: string;
      failureContext?: { source: string };
    }>;
    expect(
      tasks.find((task) => task.sliceType === "REPAIR")?.failureContext,
    ).toMatchObject({ source: "REVIEW" });
  });

  it("review 在超时后进入 FAILED", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    const originalSnapshot = GitInspector.prototype.snapshot;
    vi.spyOn(GitInspector.prototype, "snapshot").mockImplementation(
      async function (this: GitInspector) {
        await new Promise((resolve) => setTimeout(resolve, 50));
        return await originalSnapshot.call(this);
      },
    );

    const result = await core.execute({
      command: "review",
      cwd: root,
      args: { timeout: 0.01 },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      exitCode: 124,
      error: { code: "E_TIMEOUT", next: "sdd review" },
    });
  });

  (process.platform === "win32" ? it.skip : it)(
    "archives traceability and makes the change read-only",
    async () => {
      const { root, core } = await builtProject();
      await core.execute({ command: "verify", cwd: root });
      await core.execute({ command: "review", cwd: root });

      const result = await core.execute({ command: "archive", cwd: root });

      expect(result).toMatchObject({ ok: true, state: "ARCHIVED" });
      const change = join(root, ".sdd/changes/add-cancel");
      expect((await readdir(change)).sort()).toEqual([
        ".archived",
        "archive.json",
        "archive.md",
      ]);
      await expect(
        access(join(root, ".sdd/changes/add-cancel/.archived")),
      ).resolves.toBeUndefined();
      const archiveReport = await readFile(join(change, "archive.md"), "utf8");
      expect(archiveReport).toMatch(/## REQ-001[\s\S]*### REQ-001-SC-001/);
      expect(archiveReport).toContain("RED 任务：");
      expect(archiveReport).toContain("最终验证命令：");
      expect(archiveReport).toContain("## Policy Traceability");
      expect(archiveReport).toContain("deep-module-design@1.0.0");
      expect(archiveReport).toContain("tdd-task-execution@1.0.0");
      expect(archiveReport).toContain("two-axis-review@1.0.0");
      expect(archiveReport).toContain("Policy Upstream Attribution");
      expect(archiveReport).toContain("Loop Run ID:");
      expect(
        JSON.parse(await readFile(join(change, "archive.json"), "utf8")),
      ).toMatchObject({
        schemaVersion: "2.0.0",
        changeId: "add-cancel",
        quality: { taskResults: expect.any(Array) },
      });
      expect(
        JSON.parse(
          await readFile(
            join(root, ".sdd/changes/add-cancel/.archived"),
            "utf8",
          ),
        ),
      ).toMatchObject({
        archivedAt: expect.any(String),
        stateHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
        artifactHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      });
      expect(await core.execute({ command: "build", cwd: root })).toMatchObject(
        {
          ok: false,
          error: { code: "E_ARCHIVED_READONLY" },
        },
      );
    },
  );

  it("archive 会重验追踪证据并阻止 verify 后篡改结果", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    await core.execute({ command: "review", cwd: root });
    const change = join(root, ".sdd/changes/add-cancel");
    const resultPath = join(change, "task-results.json");
    const results = JSON.parse(await readFile(resultPath, "utf8"));
    results[0].modifiedFiles = [];
    await writeFile(
      resultPath,
      `${JSON.stringify(results, null, 2)}\n`,
      "utf8",
    );

    const archived = await core.execute({ command: "archive", cwd: root });

    expect(archived).toMatchObject({
      ok: false,
      error: { code: "E_MISSING_ARTIFACT" },
    });
    await expect(access(join(change, ".archived"))).rejects.toThrow();
  });

  it("review 后源码变化会阻止 archive", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    await core.execute({ command: "review", cwd: root });
    await writeFile(
      join(root, "src/order.ts"),
      "export const order = { changed: true };\n",
    );

    expect(await core.execute({ command: "archive", cwd: root })).toMatchObject(
      {
        ok: false,
        error: { code: "E_VERIFY_REQUIRED" },
      },
    );
    await expect(
      access(join(root, ".sdd/changes/add-cancel/.archived")),
    ).rejects.toThrow();
  });

  it("追加伪造 PASS 会因报告 metadata 与唯一 Result 校验失败", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    await core.execute({ command: "review", cwd: root });
    const reportPath = join(root, ".sdd/changes/add-cancel/verify-report.md");
    await writeFile(
      reportPath,
      `${await readFile(reportPath, "utf8")}\n## Result\n\nPASS\n`,
    );

    expect(await core.execute({ command: "archive", cwd: root })).toMatchObject(
      {
        ok: false,
        error: { code: "E_VERIFY_REQUIRED" },
      },
    );
  });

  (process.platform === "win32" ? it.skip : it)(
    "marker 写入后 state 更新失败不落 FAILED，重跑后收敛",
    async () => {
      const { root, core } = await builtProject();
      await core.execute({ command: "verify", cwd: root });
      await core.execute({ command: "review", cwd: root });
      const originalUpdate = StateStore.prototype.update;
      let markerUpdateAttempts = 0;
      const updateSpy = vi
        .spyOn(StateStore.prototype, "update")
        .mockImplementation(async function (this: StateStore, updater) {
          try {
            await access(join(root, ".sdd/changes/add-cancel/.archived"));
            markerUpdateAttempts += 1;
            throw new Error("注入 marker 后 state 更新失败");
          } catch (error) {
            if (markerUpdateAttempts > 0) throw error;
          }
          return originalUpdate.call(this, updater);
        });

      expect(
        await core.execute({ command: "archive", cwd: root }),
      ).toMatchObject({ ok: false });
      expect(markerUpdateAttempts).toBeGreaterThanOrEqual(2);
      expect(
        JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
      ).not.toMatchObject({ currentPhase: "FAILED" });
      updateSpy.mockRestore();
      expect(
        await core.execute({ command: "archive", cwd: root }),
      ).toMatchObject({ ok: true, state: "ARCHIVED" });
      expect(
        JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
      ).toMatchObject({
        currentPhase: "ARCHIVED",
        inProgressPhase: null,
        failedCommand: null,
        artifacts: { traceability: "READY", archiveReport: "READY" },
      });
      expect(
        await readFile(join(root, ".sdd/logs/audit.log"), "utf8"),
      ).toContain('"command":"sdd archive"');
    },
  );

  it("重复归档会清除 marker 发布后遗留的展开制品", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    await core.execute({ command: "review", cwd: root });
    const change = join(root, ".sdd/changes/add-cancel");
    expect(await core.execute({ command: "archive", cwd: root })).toMatchObject(
      {
        ok: true,
        state: "ARCHIVED",
      },
    );
    await writeFile(join(change, "遗留制品.md"), "stale\n", "utf8");

    expect(await core.execute({ command: "archive", cwd: root })).toMatchObject(
      {
        ok: true,
        state: "ARCHIVED",
      },
    );
    expect((await readdir(change)).sort()).toEqual([
      ".archived",
      "archive.json",
      "archive.md",
    ]);
  });

  it("verify 对深层 task schema 错误返回稳定路径", async () => {
    const { root, core } = await builtProject();
    const plan = await readPlan(root);
    plan.tasks[0]!.scenarios = null;
    await writePlan(root, plan);

    const result = await core.execute({ command: "verify", cwd: root });
    expect(result).toMatchObject({
      ok: false,
      error: { code: "E_STATE_CORRUPTED" },
    });
    expect(result.error?.message).toContain("plan.json.tasks[0].scenarios");
  });

  it("review 重复执行前仍会深层校验持久化任务结果", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    await core.execute({ command: "review", cwd: root });
    const path = join(root, ".sdd/changes/add-cancel/task-results.json");
    const results = JSON.parse(await readFile(path, "utf8"));
    results[0].modifiedFiles = null;
    await writeFile(path, JSON.stringify(results));

    const result = await core.execute({ command: "review", cwd: root });
    expect(result).toMatchObject({
      ok: false,
      error: { code: "E_STATE_CORRUPTED" },
    });
    expect(result.error?.message).toContain(
      "task-results.json[0].modifiedFiles",
    );
  });

  it("verify 拒绝不存在的任务依赖", async () => {
    const { root, core } = await builtProject();
    const plan = await readPlan(root);
    plan.tasks[0]!.dependsOn = ["TASK-GHOST"];
    await writePlan(root, plan);

    const result = await core.execute({ command: "verify", cwd: root });
    expect(result).toMatchObject({
      ok: false,
      error: { code: "E_STATE_CORRUPTED" },
    });
    expect(result.error?.message).toContain("不存在依赖任务 TASK-GHOST");
  });

  it("verify 拒绝非稳定顺序的 model ID", async () => {
    const { root, core } = await builtProject();
    const path = join(root, ".sdd/changes/add-cancel/spec.json");
    const compact = JSON.parse(await readFile(path, "utf8"));
    compact.model.requirements[0].id = "REQ-999";
    await writeFile(path, JSON.stringify(compact));

    const result = await core.execute({ command: "verify", cwd: root });
    expect(result).toMatchObject({
      ok: false,
      error: { code: "E_STATE_CORRUPTED" },
    });
    expect(result.error?.message).toContain("必须为 REQ-001");
  });

  (process.platform === "win32" ? it.skip : it)(
    "即使 state 被误改，只要存在 .archived 也拒绝再次写入",
    async () => {
      const { root, core } = await builtProject();
      await core.execute({ command: "verify", cwd: root });
      await core.execute({ command: "review", cwd: root });
      await core.execute({ command: "archive", cwd: root });

      const statePath = join(root, ".sdd/state.json");
      const state = JSON.parse(await readFile(statePath, "utf8")) as {
        currentPhase: string;
        suggestedCommand: string | null;
      };
      await writeFile(
        statePath,
        `${JSON.stringify(
          {
            ...state,
            currentPhase: "BUILD_READY",
            suggestedCommand: "sdd verify",
          },
          null,
          2,
        )}\n`,
        "utf8",
      );

      expect(
        await core.execute({ command: "verify", cwd: root }),
      ).toMatchObject({
        ok: false,
        error: { code: "E_ARCHIVED_READONLY" },
      });
    },
  );

  (process.platform === "win32" ? it.skip : it)(
    "archive 拒绝 artifactHash 与归档制品不一致的 marker",
    async () => {
      const { root, core } = await builtProject();
      await core.execute({ command: "verify", cwd: root });
      await core.execute({ command: "review", cwd: root });
      await core.execute({ command: "archive", cwd: root });
      const markerPath = join(root, ".sdd/changes/add-cancel/.archived");
      const marker = JSON.parse(await readFile(markerPath, "utf8"));
      marker.artifactHash = `sha256:${"0".repeat(64)}`;
      await writeFile(markerPath, JSON.stringify(marker));

      expect(
        await core.execute({ command: "archive", cwd: root }),
      ).toMatchObject({
        ok: false,
        error: { code: "E_STATE_CORRUPTED" },
      });
    },
  );

  it("当 --change 与当前活动变更不一致时拒绝执行 archive", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    await core.execute({ command: "review", cwd: root });

    const result = await core.execute({
      command: "archive",
      cwd: root,
      args: { changeId: "other-change" },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "REVIEW_READY",
      error: { code: "E_MISSING_CHANGE" },
    });
  });

  it("archive 在超时后进入 FAILED", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    await core.execute({ command: "review", cwd: root });
    const originalWrite = ArtifactWriter.prototype.write;
    vi.spyOn(ArtifactWriter.prototype, "write").mockImplementation(
      async function (
        this: ArtifactWriter,
        ...args: Parameters<ArtifactWriter["write"]>
      ) {
        await new Promise((resolve) => setTimeout(resolve, 50));
        return await originalWrite.apply(this, args);
      },
    );

    const result = await core.execute({
      command: "archive",
      cwd: root,
      args: { timeout: 0.01 },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      exitCode: 124,
      error: { code: "E_TIMEOUT", next: "sdd archive" },
    });
  }, 15_000);

  (process.platform === "win32" ? it.skip : it)(
    "allows a new change after archive and references the archived change",
    async () => {
      const { root, core } = await builtProject();
      await core.execute({ command: "verify", cwd: root });
      await core.execute({ command: "review", cwd: root });
      await core.execute({ command: "archive", cwd: root });

      const result = await core.execute({
        command: "new",
        cwd: root,
        args: {
          requirement:
            "Extend authenticated order cancellation with an API audit endpoint, authorization, error handling, logging, and automated tests.",
          changeId: "extend-cancel",
        },
      });

      expect(result).toMatchObject({
        ok: true,
        state: "SPEC_READY",
        changeId: "extend-cancel",
      });
      expect(
        JSON.parse(
          await readFile(
            join(root, ".sdd/changes/extend-cancel/spec.json"),
            "utf8",
          ),
        ).proposal,
      ).toContain("add-cancel");
    },
  );

  (process.platform === "win32" ? it.skip : it)(
    "persists FAILED recovery context when archive lacks required artifacts and can retry",
    async () => {
      const { root, core } = await builtProject();
      await core.execute({ command: "verify", cwd: root });
      await core.execute({ command: "review", cwd: root });
      const reviewReportPath = join(
        root,
        ".sdd/changes/add-cancel/review-report.md",
      );
      const reviewReport = await readFile(reviewReportPath, "utf8");
      await rm(reviewReportPath);

      const failed = await core.execute({ command: "archive", cwd: root });

      expect(failed).toMatchObject({
        ok: false,
        state: "FAILED",
        error: { code: "E_MISSING_ARTIFACT", next: "sdd archive" },
      });
      expect(
        JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
      ).toMatchObject({
        currentPhase: "FAILED",
        previousPhase: "REVIEW_READY",
        inProgressPhase: "ARCHIVING",
        failedCommand: "sdd archive",
        suggestedCommand: "sdd archive",
      });

      await writeFile(reviewReportPath, reviewReport, "utf8");
      expect(
        await core.execute({ command: "archive", cwd: root }),
      ).toMatchObject({
        ok: true,
        state: "ARCHIVED",
      });
    },
    15_000,
  );
});

function traceFixture(): {
  document: SpecDocument;
  tasks: TaskDefinition[];
  results: StoredTaskResult[];
} {
  const document: SpecDocument = {
    title: "追踪测试",
    requirements: [
      {
        id: "REQ-001",
        title: "需求",
        statement: "系统 MUST 支持行为",
        operation: "ADDED",
        scenarios: [
          {
            id: "REQ-001-SC-001",
            title: "场景",
            given: ["条件"],
            when: ["操作"],
            then: ["结果"],
          },
        ],
      },
    ],
  };
  const phases: TddPhase[] = ["RED", "GREEN", "REFACTOR", "VERIFY"];
  const tasks = phases.map(
    (phase, index): TaskDefinition => ({
      id: `TASK-${phase}`,
      title: `${phase} 任务`,
      phase,
      status: "DONE",
      requirements: ["REQ-001"],
      scenarios: ["REQ-001-SC-001"],
      dependsOn: index === 0 ? [] : [`TASK-${phases[index - 1]}`],
      allowedFiles: ["src/order.ts"],
      expectedNewFiles: [],
      forbiddenFiles: [],
      verification: ["npm test"],
      doneCriteria: ["完成"],
    }),
  );
  const results = phases.map(
    (phase): StoredTaskResult => ({
      taskId: `TASK-${phase}`,
      modifiedFiles: ["src/order.ts"],
      tddEvidence: [
        {
          phase,
          command: "npm test",
          passed: phase !== "RED",
          ...(phase === "RED" ? { expectedFailure: true } : {}),
          output: phase === "RED" ? "failed" : "passed",
        },
      ],
      verification:
        phase === "VERIFY"
          ? [{ command: "npm test", passed: true, output: "passed" }]
          : [],
    }),
  );
  return { document, tasks, results };
}
