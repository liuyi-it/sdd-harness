import { access, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";

const roots: string[] = [];

async function specifiedProject(): Promise<{ root: string; core: Core }> {
  const root = await mkdtemp(join(tmpdir(), "sdd-design-"));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Order service\n", "utf8");
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

    const result = await core.execute({ command: "design", cwd: root });

    expect(result).toMatchObject({
      ok: true,
      state: "DESIGN_READY",
      next: "sdd plan",
    });
    const design = await readFile(
      join(root, ".sdd/changes/add-order-cancellation/design.md"),
      "utf8",
    );
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
    expect(tasks).toContain("TASK-001");
    expect(tasks).toContain("REQ-001");
    expect(tasks).toContain("Allowed Files");
    expect(tasks).toContain("Verification");
    for (const artifact of ["test-plan.md", "context.md"]) {
      await expect(
        access(join(root, ".sdd/changes/add-order-cancellation", artifact)),
      ).resolves.toBeUndefined();
    }
    const contextPack = await readFile(
      join(root, ".sdd/context-packs/add-order-cancellation/TASK-001.md"),
      "utf8",
    );
    expect(contextPack).toContain("Allowed Files");
    expect(contextPack).toContain("Risk");
    expect(contextPack).toMatch(/Codebase Index Hash: sha256:[a-f0-9]{64}/);
    expect(contextPack).toMatch(/Source Artifact Hash: sha256:[a-f0-9]{64}/);
    expect(contextPack).toContain("Generated At:");
    expect(Buffer.byteLength(contextPack)).toBeLessThanOrEqual(30 * 1024);
    const state = JSON.parse(
      await readFile(join(root, ".sdd/state.json"), "utf8"),
    );
    expect(state.tasks).toEqual({ "TASK-001": "PENDING" });
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
      warnings: [expect.stringContaining("candidate")],
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
      warnings: [expect.stringContaining("candidate")],
    });
    await expect(
      access(
        join(root, ".sdd/changes/add-order-cancellation/tasks.md.candidate.md"),
      ),
    ).resolves.toBeUndefined();
  });
});
