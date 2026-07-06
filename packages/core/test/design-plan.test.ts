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
    const designMetadata = JSON.parse(
      await readFile(
        join(root, ".sdd/changes/add-order-cancellation/design.md.meta.json"),
        "utf8",
      ),
    ) as {
      schemaVersion: string;
      generatedBy: string;
      inputHash: string;
      artifactHash: string;
      createdAt: string;
    };
    for (const section of [
      "Current Code Structure",
      "Target Design",
      "API Changes",
      "Transaction and Idempotency",
      "Logging and Monitoring",
      "Testing Strategy",
      "Risks and Rollback",
    ]) {
      expect(design).toContain(section);
    }
    expect(design).toContain("README.md");
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

  it("creates requirement-linked tasks, test plan, context, and per-task Context Pack", async () => {
    const { root, core } = await specifiedProject();
    await core.execute({ command: "design", cwd: root });

    const result = await core.execute({ command: "plan", cwd: root });

    expect(result).toMatchObject({
      ok: true,
      state: "PLAN_READY",
      next: "sdd build",
    });
    const tasks = await readFile(
      join(root, ".sdd/changes/add-order-cancellation/tasks.md"),
      "utf8",
    );
    const tasksMetadata = JSON.parse(
      await readFile(
        join(root, ".sdd/changes/add-order-cancellation/tasks.md.meta.json"),
        "utf8",
      ),
    ) as {
      schemaVersion: string;
      generatedBy: string;
      inputHash: string;
      artifactHash: string;
      createdAt: string;
    };
    expect(tasks).toContain("TASK-001-RED");
    expect(tasks).toContain("REQ-001");
    expect(tasks).toContain("Allowed Files");
    expect(tasks).toContain("Verification");
    for (const artifact of ["test-plan.md", "context.md"]) {
      await expect(
        access(join(root, ".sdd/changes/add-order-cancellation", artifact)),
      ).resolves.toBeUndefined();
      expect(
        JSON.parse(
          await readFile(
            join(
              root,
              ".sdd/changes/add-order-cancellation",
              `${artifact}.meta.json`,
            ),
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
    }
    const contextPack = await readFile(
      join(root, ".sdd/context-packs/add-order-cancellation/TASK-001-RED.md"),
      "utf8",
    );
    expect(contextPack).toContain("Allowed Files");
    expect(contextPack).toContain("Risk");
    expect(contextPack).toContain("Project Rules");
    expect(contextPack).toContain("AGENTS.md");
    expect(contextPack).toMatch(/Codebase Index Hash: sha256:[a-f0-9]{64}/);
    expect(contextPack).toMatch(/Source Artifact Hash: sha256:[a-f0-9]{64}/);
    expect(contextPack).toMatch(/Project Rules Hash: sha256:[a-f0-9]{64}/);
    expect(contextPack).toMatch(
      /Project Conventions Hash: sha256:[a-f0-9]{64}/,
    );
    expect(contextPack).toContain("Generated At:");
    expect(Buffer.byteLength(contextPack)).toBeLessThanOrEqual(30 * 1024);
    expect(tasksMetadata).toMatchObject({
      schemaVersion: "1.0.0",
      generatedBy: "sdd-harness",
      inputHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      artifactHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      createdAt: expect.any(String),
    });
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

  it("returns already ready for unchanged input and writes a candidate when input changed", async () => {
    const { root, core } = await specifiedProject();
    await core.execute({ command: "design", cwd: root });
    const designPath = join(
      root,
      ".sdd/changes/add-order-cancellation/design.md",
    );
    const original = await readFile(designPath, "utf8");

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
      warnings: [expect.stringContaining("供人工合并")],
    });
    expect(await readFile(designPath, "utf8")).toBe(original);
    await expect(access(`${designPath}.candidate.md`)).resolves.toBeUndefined();
  });

  it("protects planned artifacts from changed inputs", async () => {
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
      warnings: [expect.stringContaining("供人工合并")],
    });
    await expect(
      access(
        join(root, ".sdd/changes/add-order-cancellation/tasks.md.candidate.md"),
      ),
    ).resolves.toBeUndefined();
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
