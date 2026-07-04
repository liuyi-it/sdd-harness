import {
  access,
  mkdtemp,
  readFile,
  readdir,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";
import { SpecEngine } from "../src/engines/spec/spec-engine.js";

// new 阶段重点验证需求不足时的澄清停顿，以及补充答案后的继续执行行为。
const roots: string[] = [];

async function initializedProject(): Promise<{ root: string; core: Core }> {
  const root = await mkdtemp(join(tmpdir(), "sdd-new-"));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Orders\n", "utf8");
  const core = new Core({ codebase: new CodebaseAdapter() });
  await core.execute({ command: "init", cwd: root });
  return { root, core };
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("sdd new", () => {
  it("pauses in CLARIFYING when a BLOCKER answer is required", async () => {
    const { root, core } = await initializedProject();

    const result = await core.execute({
      command: "new",
      cwd: root,
      args: { requirement: "增加取消功能", changeId: "add-cancel" },
    });

    expect(result).toMatchObject({
      ok: true,
      state: "CLARIFYING",
      next: "sdd new",
    });
    expect(
      await readFile(
        join(root, ".sdd/changes/add-cancel/questions.md"),
        "utf8",
      ),
    ).toContain("BLOCKER");
    const [runId] = await readdir(join(root, ".sdd/runs"));
    expect(runId).toBeDefined();
    await expect(
      access(join(root, ".sdd/runs", runId!, "input.md")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(root, ".sdd/runs", runId!, "input.md.meta.json")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(root, ".sdd/changes/add-cancel/spec.md")),
    ).rejects.toThrow();
  });

  it("continues the active change after blocker answers are supplied", async () => {
    const { root, core } = await initializedProject();
    await core.execute({
      command: "new",
      cwd: root,
      args: { requirement: "增加取消功能", changeId: "add-cancel" },
    });

    const result = await core.execute({
      command: "new",
      cwd: root,
      args: {
        answers: { "Q-001": "仅允许创建者取消未完成订单，并提供 API 和测试" },
      },
    });

    expect(result).toMatchObject({
      ok: true,
      state: "SPEC_READY",
      next: "sdd design",
    });
    const spec = await readFile(
      join(root, ".sdd/changes/add-cancel/spec.md"),
      "utf8",
    );
    expect(spec).toContain("## Requirements");
    expect(spec).toContain("REQ-001");
    expect(spec).toContain("Acceptance Criteria");
    await expect(
      access(join(root, ".sdd/changes/add-cancel/spec.md.meta.json")),
    ).resolves.toBeUndefined();
  });

  it("fails instead of guessing in non-interactive mode", async () => {
    const { root, core } = await initializedProject();

    const result = await core.execute({
      command: "new",
      cwd: root,
      args: {
        requirement: "增加取消功能",
        changeId: "add-cancel",
        nonInteractive: true,
      },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      exitCode: 6,
      error: { code: "E_UNRESOLVED_BLOCKER" },
    });
  });

  it("generates the complete spec artifact set for a sufficiently detailed requirement", async () => {
    const { root, core } = await initializedProject();
    const result = await core.execute({
      command: "new",
      cwd: root,
      args: {
        requirement:
          "Implement authenticated order cancellation for pending orders through POST /orders/:id/cancel, including authorization, conflict errors, audit logging, and automated tests.",
        changeId: "add-order-cancellation",
      },
    });

    expect(result.state).toBe("SPEC_READY");
    for (const artifact of [
      "proposal.md",
      "questions.md",
      "answers.md",
      "assumptions.md",
      "impact.md",
      "spec.md",
    ]) {
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
  });

  it("new 在规格生成超时后进入 FAILED 并记录恢复上下文", async () => {
    const { root } = await initializedProject();
    class SlowSpecEngine extends SpecEngine {
      override async generate(...args: Parameters<SpecEngine["generate"]>) {
        await new Promise((resolve) => setTimeout(resolve, 50));
        return await super.generate(...args);
      }
    }
    const core = new Core({
      codebase: new CodebaseAdapter(),
      specEngine: new SlowSpecEngine(),
    });

    const result = await core.execute({
      command: "new",
      cwd: root,
      args: {
        requirement:
          "Implement authenticated order cancellation for pending orders through POST /orders/:id/cancel, including authorization, conflict errors, audit logging, and automated tests.",
        changeId: "add-order-cancellation",
        timeout: 0.01,
      },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      exitCode: 124,
      error: { code: "E_TIMEOUT", next: "sdd new" },
    });
  });

  it("new 在收到中断后进入 PAUSED 并记录恢复上下文", async () => {
    const { root } = await initializedProject();
    class SlowSpecEngine extends SpecEngine {
      override async generate(...args: Parameters<SpecEngine["generate"]>) {
        await new Promise((resolve) => setTimeout(resolve, 50));
        return await super.generate(...args);
      }
    }
    const core = new Core({
      codebase: new CodebaseAdapter(),
      specEngine: new SlowSpecEngine(),
    });
    const controller = new AbortController();
    setTimeout(() => controller.abort(), 10);

    const result = await core.execute({
      command: "new",
      cwd: root,
      signal: controller.signal,
      args: {
        requirement:
          "Implement authenticated order cancellation for pending orders through POST /orders/:id/cancel, including authorization, conflict errors, audit logging, and automated tests.",
        changeId: "add-order-cancellation",
      },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "PAUSED",
      exitCode: 130,
      error: { code: "E_INTERRUPTED", next: "sdd new" },
    });
    expect(
      JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
    ).toMatchObject({
      currentPhase: "PAUSED",
      interruptedCommand: "sdd new",
      previousPhase: "INDEX_READY",
      inProgressPhase: "NEW_STARTED",
      suggestedCommand: "sdd new",
      lastError: "E_INTERRUPTED",
    });
  });
});
