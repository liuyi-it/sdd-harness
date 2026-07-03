import { access, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";

// 这组测试把 verify/review/archive 串起来，验证“完成证据”最终能沉淀成可追踪归档。
const roots: string[] = [];

async function builtProject(): Promise<{ root: string; core: Core }> {
  const root = await mkdtemp(join(tmpdir(), "sdd-quality-"));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Orders\n", "utf8");
  const core = new Core({
    codebase: new CodebaseAdapter(),
    taskExecutor: {
      execute: vi.fn().mockResolvedValue({
        modifiedFiles: ["src/order.ts", "test/order.test.ts"],
        verification: [{ command: "npm test", passed: true, output: "passed" }],
      }),
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
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("quality commands", () => {
  it("verifies requirement, task, acceptance, and test coverage", async () => {
    const { root, core } = await builtProject();
    const result = await core.execute({ command: "verify", cwd: root });
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
  });

  it("reviews scope and implementation evidence", async () => {
    const { root, core } = await builtProject();
    await core.execute({ command: "verify", cwd: root });
    const result = await core.execute({ command: "review", cwd: root });
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
    await expect(
      access(join(root, ".sdd/changes/add-cancel/.archived")),
    ).resolves.toBeUndefined();
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
});
