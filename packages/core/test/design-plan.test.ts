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

import { afterEach, describe, expect, it } from "vitest";

import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";
import { TddEngine } from "../src/engines/tdd/tdd-engine.js";
import { ArtifactWriter } from "../src/artifacts/artifact-writer.js";

// 这组测试验证 design/plan 是否会基于真实索引上下文生成稳定制品，并保持幂等/candidate 语义。
const roots: string[] = [];

async function specifiedProject(): Promise<{ root: string; core: Core }> {
  const root = await mkdtemp(join(tmpdir(), "sdd-design-"));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Order service\n", "utf8");
  await mkdir(join(root, "src"));
  await mkdir(join(root, "test"));
  await writeFile(
    join(root, "package.json"),
    '{"scripts":{"test":"vitest"}}\n',
  );
  await writeFile(join(root, "src/order.ts"), "export const order = {};\n");
  await writeFile(join(root, "test/order.test.ts"), "// order tests\n");
  const core = new Core({ codebase: new CodebaseAdapter() });
  await core.execute({ command: "init", cwd: root });
  await core.execute({
    command: "new",
    cwd: root,
    args: {
      requirement:
        "Implement authenticated order cancellation for pending orders through an API endpoint, including authorization, conflict errors, audit logging, and automated tests.",
      changeId: "add-order-cancellation",
    },
  });
  return { root, core };
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("design and plan", () => {
  it("generates a design grounded in indexed codebase context", async () => {
    const { root, core } = await specifiedProject();

    const result = await core.execute({
      command: "design",
      cwd: root,
      args: { changeId: "add-order-cancellation" },
    });

    expect(result).toMatchObject({
      ok: true,
      state: "DESIGN_READY",
      next: "sdd plan",
    });
    const design = await readFile(
      join(root, ".sdd/changes/add-order-cancellation/design.md"),
      "utf8",
    );
    const designMetadata = await new ArtifactWriter().metadata(
      join(root, ".sdd/changes/add-order-cancellation/design.md"),
    );
    for (const section of [
      "Current Code Structure",
      "Target Design",
      "API Changes",
      "Interfaces and Contracts",
      "Transaction and Idempotency",
      "Logging and Monitoring",
      "Testing Strategy",
      "Test Seams",
      "Risks and Rollback",
    ]) {
      expect(design).toContain(section);
    }
    expect(design).toContain("README.md");
    expect(design).toContain("## Phase Policy");
    expect(design).toContain("deep-module-design");
    expect(design).toContain("design-it-twice");
    expect(design).toContain("方案 A");
    expect(design).toContain("方案 B");
    expect(designMetadata).toMatchObject({
      schemaVersion: "1.0.0",
      generatedBy: "sdd-harness",
      inputHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      artifactHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      createdAt: expect.any(String),
    });
  });

  it("当 --change 与当前活动变更不一致时拒绝执行 design", async () => {
    const { root, core } = await specifiedProject();

    const result = await core.execute({
      command: "design",
      cwd: root,
      args: { changeId: "other-change" },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "SPEC_READY",
      error: { code: "E_MISSING_CHANGE" },
    });
  });

  it("creates a compact plan and defers Context Pack generation until build", async () => {
    const { root, core } = await specifiedProject();
    await core.execute({ command: "design", cwd: root });

    const result = await core.execute({ command: "plan", cwd: root });

    expect(result).toMatchObject({
      ok: true,
      state: "PLAN_READY",
      next: "sdd build next",
    });
    const planPath = join(
      root,
      ".sdd/changes/add-order-cancellation/plan.json",
    );
    const plan = JSON.parse(await readFile(planPath, "utf8"));
    const tasks = plan.tasksMarkdown as string;
    const tasksMetadata = await new ArtifactWriter().metadata(planPath);
    expect(tasks).toContain("TASK-001-RED");
    expect(tasks).toContain("REQ-001");
    expect(tasks).toContain("Allowed Files");
    expect(tasks).toContain("Verification");
    expect(plan.testPlan).toContain("Test Plan");
    expect(plan.context).toBeTypeOf("string");
    await expect(
      access(
        join(root, ".sdd/context-packs/add-order-cancellation/TASK-001-RED.md"),
      ),
    ).rejects.toThrow();
    expect(tasksMetadata).toMatchObject({
      schemaVersion: "1.0.0",
      generatedBy: "sdd-harness",
      inputHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      artifactHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      createdAt: expect.any(String),
    });
    const taskDefinitions = plan.tasks as Array<{
      policyRefs?: Array<{ id: string }>;
    }>;
    expect(taskDefinitions[0]?.policyRefs?.map(({ id }) => id)).toEqual([
      "core-authority",
      "tracer-bullet-planning",
      "expand-contract-migration",
      "minimal-implementation",
    ]);
    const state = JSON.parse(
      await readFile(join(root, ".sdd/state.json"), "utf8"),
    );
    expect(Object.keys(state.tasks)).toHaveLength(16);
    expect(Object.values(state.tasks)).toEqual(
      expect.arrayContaining(["PENDING"]),
    );
  });

  it("rejects plan before design", async () => {
    const { root, core } = await specifiedProject();
    const result = await core.execute({ command: "plan", cwd: root });
    expect(result).toMatchObject({
      ok: false,
      state: "SPEC_READY",
      error: { code: "E_INVALID_PHASE_COMMAND", next: "sdd design" },
    });
  });

  it("returns already ready for unchanged input and regenerates with existing design merge when input changed", async () => {
    const { root, core } = await specifiedProject();
    await core.execute({ command: "design", cwd: root });
    const designPath = join(
      root,
      ".sdd/changes/add-order-cancellation/design.md",
    );

    expect(await core.execute({ command: "design", cwd: root })).toMatchObject({
      ok: true,
      state: "DESIGN_READY",
      data: { alreadyReady: true },
    });
    await writeFile(
      join(root, ".sdd/changes/add-order-cancellation/spec.md"),
      "# Spec changed by user\n\n## Requirements\n\n### REQ-001\n",
      "utf8",
    );

    const changed = await core.execute({ command: "design", cwd: root });

    expect(changed).toMatchObject({
      ok: true,
      state: "DESIGN_READY",
    });
    // 输入变化后，design 使用已有 design 合并重新生成，不再使用 candidate 文件
    const newDesign = await readFile(designPath, "utf8");
    expect(newDesign).toContain("Current Code Structure");
    await expect(access(`${designPath}.candidate.md`)).rejects.toThrow();
  });

  it("regenerates plan artifacts with merge when input changes", async () => {
    const { root, core } = await specifiedProject();
    await core.execute({ command: "design", cwd: root });
    await core.execute({ command: "plan", cwd: root });
    expect(await core.execute({ command: "plan", cwd: root })).toMatchObject({
      ok: true,
      state: "PLAN_READY",
      data: { alreadyReady: true },
    });
    await writeFile(
      join(root, ".sdd/changes/add-order-cancellation/design.md"),
      "# User-adjusted design\n",
      "utf8",
    );

    const changed = await core.execute({ command: "plan", cwd: root });

    expect(changed).toMatchObject({
      ok: true,
      state: "PLAN_READY",
    });
    await expect(
      access(
        join(root, ".sdd/changes/add-order-cancellation/plan.candidate.json"),
      ),
    ).rejects.toThrow();
  });

  it("传入 --force 时直接覆盖 design 制品而不是生成 candidate", async () => {
    const { root, core } = await specifiedProject();
    await core.execute({ command: "design", cwd: root });
    const designPath = join(
      root,
      ".sdd/changes/add-order-cancellation/design.md",
    );
    const original = await readFile(designPath, "utf8");
    await writeFile(
      join(root, ".sdd/changes/add-order-cancellation/spec.md"),
      "# Spec changed by user\n\n## Requirements\n\n### REQ-001\n",
      "utf8",
    );

    const changed = await core.execute({
      command: "design",
      cwd: root,
      args: { force: true },
    });

    expect(changed).toMatchObject({
      ok: true,
      state: "DESIGN_READY",
      next: "sdd plan",
    });
    expect(await readFile(designPath, "utf8")).not.toBe(original);
    await expect(access(`${designPath}.candidate.md`)).rejects.toThrow();
  });

  it("persists FAILED recovery context when design is missing required artifacts and can retry", async () => {
    const { root, core } = await specifiedProject();
    const summaryPath = join(root, ".sdd/index/codebase-summary.md");
    const originalSummary = await readFile(summaryPath, "utf8");
    await rm(summaryPath);

    const failed = await core.execute({ command: "design", cwd: root });

    expect(failed).toMatchObject({
      ok: false,
      state: "FAILED",
      error: { code: "E_MISSING_ARTIFACT", next: "sdd design" },
    });
    expect(
      JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
    ).toMatchObject({
      currentPhase: "FAILED",
      previousPhase: "SPEC_READY",
      inProgressPhase: "DESIGNING",
      failedCommand: "sdd design",
      suggestedCommand: "sdd design",
    });

    await writeFile(summaryPath, originalSummary, "utf8");
    expect(await core.execute({ command: "design", cwd: root })).toMatchObject({
      ok: true,
      state: "DESIGN_READY",
      next: "sdd plan",
    });
  });

  it("design 在生成超时后进入 FAILED", async () => {
    const { root } = await specifiedProject();
    class SlowTddEngine extends TddEngine {
      override async generateDesign(
        ...args: Parameters<TddEngine["generateDesign"]>
      ) {
        await new Promise((resolve) => setTimeout(resolve, 50));
        return await super.generateDesign(...args);
      }
    }
    const core = new Core({
      codebase: new CodebaseAdapter(),
      tddEngine: new SlowTddEngine(),
    });

    const result = await core.execute({
      command: "design",
      cwd: root,
      args: { timeout: 0.01 },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      exitCode: 124,
      error: { code: "E_TIMEOUT", next: "sdd design" },
    });
  });

  it("plan 在生成超时后进入 FAILED", async () => {
    const { root, core: seededCore } = await specifiedProject();
    await seededCore.execute({ command: "design", cwd: root });
    class SlowTddEngine extends TddEngine {
      override async generatePlan(
        ...args: Parameters<TddEngine["generatePlan"]>
      ) {
        await new Promise((resolve) => setTimeout(resolve, 50));
        return await super.generatePlan(...args);
      }
    }
    const core = new Core({
      codebase: new CodebaseAdapter(),
      tddEngine: new SlowTddEngine(),
    });

    const result = await core.execute({
      command: "plan",
      cwd: root,
      args: { timeout: 0.01 },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      exitCode: 124,
      error: { code: "E_TIMEOUT", next: "sdd plan" },
    });
  });
});
