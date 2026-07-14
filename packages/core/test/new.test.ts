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
import { ArtifactWriter } from "../src/artifacts/artifact-writer.js";
import { SpecEngine } from "../src/engines/spec/spec-engine.js";

// new 阶段重点验证需求不足时的澄清停顿，以及补充答案后的继续执行行为。
const roots: string[] = [];

async function initializedProject(): Promise<{ root: string; core: Core }> {
  const root = await mkdtemp(join(tmpdir(), "sdd-new-"));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Orders\n", "utf8");
  await writeFile(join(root, "package.json"), '{"name":"orders"}\n', "utf8");
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
    const compact = JSON.parse(
      await readFile(join(root, ".sdd/changes/add-cancel/spec.json"), "utf8"),
    );
    expect(compact).toMatchObject({
      schemaVersion: "2.0.0",
      status: "CLARIFYING",
    });
    expect(compact.questions).toContain("BLOCKER");
    const [runId] = await readdir(join(root, ".sdd/runs"));
    expect(runId).toBeDefined();
    await expect(
      access(join(root, ".sdd/runs", runId!, "input.md")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(root, ".sdd/artifacts.json")),
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
        answers: {
          "Q-ACTOR": "仅允许创建者，其他用户必须被拒绝",
          "Q-ACTION": "通过 API 取消订单",
          "Q-RESULT": "仅允许取消未完成订单，成功后状态为已取消",
          "Q-FAILURE": "重复取消返回冲突错误",
          "Q-TEST": "覆盖成功、未授权和冲突自动化测试",
        },
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
    expect(spec).toContain("## ADDED Requirements");
    expect(spec).toContain("### Requirement:");
    expect(spec).toContain("#### Scenario:");
    await expect(
      access(join(root, ".sdd/artifacts.json")),
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
    for (const artifact of ["spec.md", "spec.json"]) {
      await expect(
        access(join(root, ".sdd/changes/add-order-cancellation", artifact)),
      ).resolves.toBeUndefined();
      expect(
        await new ArtifactWriter().metadata(
          join(root, ".sdd/changes/add-order-cancellation", artifact),
        ),
      ).toMatchObject({
        schemaVersion: "1.0.0",
        generatedBy: "sdd-harness",
        inputHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
        artifactHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
        createdAt: expect.any(String),
      });
    }

    const change = join(root, ".sdd/changes/add-order-cancellation");
    const compact = JSON.parse(
      await readFile(join(change, "spec.json"), "utf8"),
    );
    expect(compact.proposal).toContain("bounded-clarification");
    expect(compact.proposal).toContain("spec-authoring");
    const spec = await readFile(join(change, "spec.md"), "utf8");
    expect(compact.delta.trimEnd()).toBe(spec.trimEnd());
    const model = compact.model as { requirements: unknown[] };
    expect(model.requirements.length).toBeGreaterThanOrEqual(3);
  });

  it("protects manually edited structured artifacts and honors force", async () => {
    const requirement =
      "Implement authenticated order cancellation through an API endpoint with authorization, conflict errors, audit logging, and automated tests.";
    const first = await initializedProject();
    const firstChange = join(first.root, ".sdd/changes/protected-spec");
    const writer = new ArtifactWriter();
    await writer.write(join(firstChange, "spec.md"), "# old spec", {});
    await writer.write(
      join(firstChange, "spec.json"),
      JSON.stringify({
        schemaVersion: "2.0.0",
        status: "READY",
        requirement,
        proposal: "old",
        impact: "old",
        questions: "old",
        delta: "# 人工修改",
        model: { title: "old", requirements: [] },
      }),
      {},
    );

    const protectedResult = await first.core.execute({
      command: "new",
      cwd: first.root,
      args: { requirement, changeId: "protected-spec" },
    });

    expect(protectedResult).toMatchObject({
      state: "SPEC_READY",
    });
    // 没有 candidate 文件，直接就地合并覆盖
    await expect(
      access(join(firstChange, "spec.json.candidate.json")),
    ).rejects.toThrow();

    const second = await initializedProject();
    const secondChange = join(second.root, ".sdd/changes/forced-spec");
    await writer.write(join(secondChange, "spec.md"), "# old spec", {});
    await writer.write(
      join(secondChange, "spec.json"),
      JSON.stringify({
        schemaVersion: "2.0.0",
        status: "READY",
        requirement,
        proposal: "old",
        impact: "old",
        questions: "old",
        delta: "# 人工修改",
        model: { title: "old", requirements: [] },
      }),
      {},
    );

    const forcedResult = await second.core.execute({
      command: "new",
      cwd: second.root,
      args: { requirement, changeId: "forced-spec", force: true },
    });

    expect(forcedResult.state).toBe("SPEC_READY");
    const forced = JSON.parse(
      await readFile(join(secondChange, "spec.json"), "utf8"),
    );
    expect(forced.delta).toContain("## ADDED Requirements");
    expect(forced.model).toMatchObject({ title: "Requested Change" });
  });

  it("repeats structured artifact generation idempotently", async () => {
    const { root, core } = await initializedProject();
    const args = {
      requirement:
        "Implement authenticated order cancellation through an API endpoint with authorization, conflict errors, audit logging, and automated tests.",
      changeId: "idempotent-spec",
    };
    await core.execute({ command: "new", cwd: root, args });
    const artifactPath = join(root, ".sdd/changes/idempotent-spec/spec.json");

    const repeated = await core.execute({ command: "new", cwd: root, args });

    expect(repeated).toMatchObject({ ok: true, state: "SPEC_READY" });
    const after = await new ArtifactWriter().metadata(artifactPath);
    expect(after).toMatchObject({
      schemaVersion: "1.0.0",
      generatedBy: "sdd-harness",
      inputHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
      artifactHash: expect.stringMatching(/^sha256:[a-f0-9]{64}$/),
    });
  });

  it("rejects a different requirement when rerunning SPEC_READY", async () => {
    const { root, core } = await initializedProject();
    const requirement =
      "Implement authenticated order cancellation through an API endpoint with authorization, conflict errors, audit logging, and automated tests.";
    await core.execute({
      command: "new",
      cwd: root,
      args: { requirement, changeId: "stable-input" },
    });

    const result = await core.execute({
      command: "new",
      cwd: root,
      args: {
        requirement: `${requirement} Changed.`,
        changeId: "stable-input",
      },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "SPEC_READY",
      error: { code: "E_ACTIVE_CHANGE_EXISTS" },
    });
  });

  it("isolates codebase summary from non-impact artifact metadata", async () => {
    const { root, core } = await initializedProject();
    const args = {
      requirement:
        "Implement authenticated order cancellation through an API endpoint with authorization, conflict errors, audit logging, and automated tests.",
      changeId: "summary-isolation",
    };
    await core.execute({ command: "new", cwd: root, args });
    const change = join(root, ".sdd/changes/summary-isolation");
    const metadata = async (name: string) =>
      (await new ArtifactWriter().metadata(join(change, name)))!;
    const before = Object.fromEntries(
      await Promise.all(
        ["spec.md", "spec.json"].map(async (name) => [
          name,
          await metadata(name),
        ]),
      ),
    );
    await writeFile(
      join(root, ".sdd/index/codebase-summary.md"),
      "# changed untrusted summary\n",
      "utf8",
    );

    await core.execute({ command: "new", cwd: root, args });

    expect((await metadata("spec.md")).inputHash).toBe(
      (before["spec.md"] as { inputHash: string }).inputHash,
    );
    expect((await metadata("spec.json")).inputHash).not.toBe(
      (before["spec.json"] as { inputHash: string }).inputHash,
    );
    await expect(
      access(join(change, "spec.json.candidate.json")),
    ).rejects.toThrow();
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
