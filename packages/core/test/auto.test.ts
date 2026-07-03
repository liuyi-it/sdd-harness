import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";

const roots: string[] = [];

async function initializedCore(): Promise<{ root: string; core: Core }> {
  const root = await mkdtemp(join(tmpdir(), "sdd-auto-"));
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
  return { root, core };
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("sdd auto", () => {
  it("runs the complete workflow from a detailed requirement", async () => {
    const { root, core } = await initializedCore();
    const result = await core.execute({
      command: "auto",
      cwd: root,
      args: {
        requirement:
          "Implement authenticated order cancellation through an API endpoint with authorization, errors, logging, and automated tests.",
        changeId: "add-cancel",
      },
    });
    expect(result).toMatchObject({ ok: true, state: "ARCHIVED", exitCode: 0 });
  });

  it("stops at CLARIFYING rather than entering build", async () => {
    const { root, core } = await initializedCore();
    const result = await core.execute({
      command: "auto",
      cwd: root,
      args: { requirement: "增加取消", changeId: "add-cancel" },
    });
    expect(result).toMatchObject({
      ok: true,
      state: "CLARIFYING",
      next: "sdd new",
    });
  });
});
