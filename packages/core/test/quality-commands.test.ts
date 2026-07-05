import { execFileSync } from "node:child_process";
import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import { ArtifactWriter } from "../src/artifacts/artifact-writer.js";
import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";
import { GitInspector } from "../src/git/git-inspector.js";
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
      JSON.parse(
        await readFile(
          join(root, ".sdd/changes/add-cancel/verify-report.md.meta.json"),
          "utf8",
        ),
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

  it("fails verify when post-build drift introduces files outside task results", async () => {
    const { root, core } = await builtProject();
    await writeFile(join(root, "notes.txt"), "manual drift\n", "utf8");

    const result = await core.execute({ command: "verify", cwd: root });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      error: { code: "E_VERIFY_FAILED", next: "sdd verify" },
    });
    expect(
      await readFile(
        join(root, ".sdd/changes/add-cancel/verify-report.md"),
        "utf8",
      ),
    ).toContain("未跟踪到任务结果的变更文件：notes.txt");
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
      error: { code: "E_VERIFY_FAILED" },
    });
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

  it("spec.model.json 损坏时 verify 不回退到 Markdown", async () => {
    const { root, core } = await builtProject();
    await writeFile(
      join(root, ".sdd/changes/add-cancel/spec.model.json"),
      '{"title":"损坏","requirements":"不是数组"}\n',
      "utf8",
    );

    expect(await core.execute({ command: "verify", cwd: root })).toMatchObject({
      ok: false,
      error: { code: "E_STATE_CORRUPTED" },
    });
  });

  it("verify 拒绝任务引用不存在的 Scenario", async () => {
    const { root, core } = await builtProject();
    const path = join(root, ".sdd/changes/add-cancel/tasks.json");
    const tasks = JSON.parse(await readFile(path, "utf8"));
    tasks[0].scenarios = ["SCN-GHOST"];
    await writeFile(path, `${JSON.stringify(tasks, null, 2)}\n`, "utf8");

    const result = await core.execute({ command: "verify", cwd: root });

    expect(result).toMatchObject({
      ok: false,
      error: { code: "E_VERIFY_FAILED" },
    });
    expect(result.error?.message).toContain("SCN-GHOST");
  });

  it("legacy：缺少 spec.model.json 时兼容严格 Markdown 规格", async () => {
    const { root, core } = await builtProject();
    const change = join(root, ".sdd/changes/add-cancel");
    await rm(join(change, "spec.model.json"));
    await writeFile(
      join(change, "spec.md"),
      [
        "# Legacy Spec",
        "",
        "### REQ-001: 旧格式需求",
        "",
        "#### Scenario: 旧格式场景",
        "- GIVEN 条件成立",
        "- WHEN 执行操作",
        "- THEN 返回结果",
        "",
        "### REQ-002: 第二个旧格式需求",
        "",
        "#### Scenario: 第二个旧格式场景",
        "- GIVEN 第二个条件成立",
        "- WHEN 执行第二个操作",
        "- THEN 返回第二个结果",
      ].join("\n"),
      "utf8",
    );

    const result = await core.execute({ command: "verify", cwd: root });
    expect(result.error).toBeUndefined();
    expect(result).toMatchObject({
      ok: true,
      state: "VERIFY_READY",
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
      JSON.parse(
        await readFile(
          join(root, ".sdd/changes/add-cancel/review-report.md.meta.json"),
          "utf8",
        ),
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

  it("fails review when verify之后又出现无关改动", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    await writeFile(join(root, "notes.txt"), "manual drift\n", "utf8");

    const result = await core.execute({ command: "review", cwd: root });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      error: { code: "E_REVIEW_FAILED", next: "sdd review" },
    });
    expect(
      await readFile(
        join(root, ".sdd/changes/add-cancel/review-report.md"),
        "utf8",
      ),
    ).toContain("未跟踪到任务结果的变更文件：notes.txt");
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

  it("archives traceability and makes the change read-only", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    await core.execute({ command: "review", cwd: root });

    const result = await core.execute({ command: "archive", cwd: root });

    expect(result).toMatchObject({ ok: true, state: "ARCHIVED" });
    await expect(
      access(join(root, ".sdd/changes/add-cancel/traceability.md")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(root, ".sdd/changes/add-cancel/archive-report.md")),
    ).resolves.toBeUndefined();
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/changes/add-cancel/traceability.md.meta.json"),
          "utf8",
        ),
      ),
    ).toMatchObject({
      schemaVersion: "1.0.0",
      generatedBy: "sdd-harness",
      inputHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      artifactHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      createdAt: expect.any(String),
    });
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/changes/add-cancel/archive-report.md.meta.json"),
          "utf8",
        ),
      ),
    ).toMatchObject({
      schemaVersion: "1.0.0",
      generatedBy: "sdd-harness",
      inputHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      artifactHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      createdAt: expect.any(String),
    });
    await expect(
      access(join(root, ".sdd/changes/add-cancel/.archived")),
    ).resolves.toBeUndefined();
    const traceability = await readFile(
      join(root, ".sdd/changes/add-cancel/traceability.md"),
      "utf8",
    );
    expect(traceability).toMatch(/## REQ-001[\s\S]*### REQ-001-SC-001/);
    expect(traceability).toContain("RED 任务：");
    expect(traceability).toContain("最终验证命令：");
    expect(
      JSON.parse(
        await readFile(join(root, ".sdd/changes/add-cancel/.archived"), "utf8"),
      ),
    ).toMatchObject({
      archivedAt: expect.any(String),
      stateHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      artifactHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
    });
    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: false,
      error: { code: "E_ARCHIVED_READONLY" },
    });
  });

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

  it("即使 state 被误改，只要存在 .archived 也拒绝再次写入", async () => {
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

    expect(await core.execute({ command: "verify", cwd: root })).toMatchObject({
      ok: false,
      error: { code: "E_ARCHIVED_READONLY" },
    });
  });

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

  it("allows a new change after archive and references the archived change", async () => {
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
      await readFile(
        join(root, ".sdd/changes/extend-cancel/proposal.md"),
        "utf8",
      ),
    ).toContain("add-cancel");
  });

  it("persists FAILED recovery context when archive lacks required artifacts and can retry", async () => {
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
    expect(await core.execute({ command: "archive", cwd: root })).toMatchObject(
      {
        ok: true,
        state: "ARCHIVED",
      },
    );
  }, 15_000);
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
