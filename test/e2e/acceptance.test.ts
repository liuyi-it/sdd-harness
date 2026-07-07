import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import { ClaudeCodeAdapter } from "../../packages/claude-code-plugin/src/adapter.js";
import { CodebaseAdapter } from "../../packages/core/src/codebase/codebase-adapter.js";
import { Core } from "../../packages/core/src/core.js";
import { CodexAdapter } from "../../packages/codex-plugin/src/adapter.js";

const roots: string[] = [];
const detailedRequirement =
  "Implement authenticated order cancellation through an API endpoint with authorization, conflict errors, audit logging, and automated tests.";
const roughRequirement = "增加取消";

async function fixture(name: string): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), `sdd-acceptance-${name}-`));
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
  return root;
}

function core(): Core {
  return new Core({
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
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("MVP acceptance", () => {
  it("matches manual phase workflow outputs across Claude Code and Codex", async () => {
    const claudeRoot = await fixture("manual-claude");
    const codexRoot = await fixture("manual-codex");
    const claude = new ClaudeCodeAdapter(core());
    const codex = new CodexAdapter(core());

    expect(await claude.execute("/sdd.init", claudeRoot)).toMatchObject({
      ok: true,
      state: "INDEX_READY",
      next: "sdd new",
    });
    expect(await codex.execute("sdd init", codexRoot)).toMatchObject({
      ok: true,
      state: "INDEX_READY",
      next: "sdd new",
    });
    expect(await claude.execute("/sdd.status", claudeRoot)).toMatchObject({
      ok: true,
      state: "INDEX_READY",
      next: "sdd new",
    });
    expect(await codex.execute("sdd status", codexRoot)).toMatchObject({
      ok: true,
      state: "INDEX_READY",
      next: "sdd new",
    });

    await expect(access(join(claudeRoot, ".sdd"))).resolves.toBeUndefined();
    await expect(access(join(codexRoot, ".sdd"))).resolves.toBeUndefined();
    await expect(
      access(join(claudeRoot, "CLAUDE.md")),
    ).resolves.toBeUndefined();
    await expect(access(join(codexRoot, "AGENTS.md"))).resolves.toBeUndefined();

    expect(
      await claude.execute(
        `/sdd.new "${detailedRequirement}" --change add-cancel`,
        claudeRoot,
      ),
    ).toMatchObject({ ok: true, state: "SPEC_READY", next: "sdd design" });
    expect(
      await codex.execute(
        `sdd new "${detailedRequirement}" --change add-cancel`,
        codexRoot,
      ),
    ).toMatchObject({ ok: true, state: "SPEC_READY", next: "sdd design" });

    for (const command of [
      { claude: "/sdd.design", codex: "sdd design", next: "sdd plan" },
      { claude: "/sdd.plan", codex: "sdd plan", next: "sdd build" },
      { claude: "/sdd.build", codex: "sdd build", next: "sdd verify" },
      { claude: "/sdd.verify", codex: "sdd verify", next: "sdd review" },
      { claude: "/sdd.review", codex: "sdd review", next: "sdd archive" },
      { claude: "/sdd.archive", codex: "sdd archive", next: undefined },
    ]) {
      const claudeResult = await claude.execute(command.claude, claudeRoot);
      const codexResult = await codex.execute(command.codex, codexRoot);
      expect(claudeResult.ok).toBe(true);
      expect(codexResult.ok).toBe(true);
      if (command.next !== undefined) {
        expect(claudeResult.next).toBe(command.next);
        expect(codexResult.next).toBe(command.next);
      }
    }

    for (const file of [
      "spec.md",
      "design.md",
      "tasks.md",
      "verify-report.md",
      "review-report.md",
      "archive-report.md",
      "traceability.md",
      "task-results.json",
    ]) {
      expect(
        await readFile(
          join(claudeRoot, ".sdd/changes/add-cancel", file),
          "utf8",
        ),
      ).toBe(
        await readFile(
          join(codexRoot, ".sdd/changes/add-cancel", file),
          "utf8",
        ),
      );
    }
    expect(
      await readFile(
        join(claudeRoot, ".sdd/changes/add-cancel/archive-report.md"),
        "utf8",
      ),
    ).toContain("- finalHead: (unavailable)");

    expect(
      await readdir(join(claudeRoot, ".sdd/context-packs/add-cancel")),
    ).toEqual(await readdir(join(codexRoot, ".sdd/context-packs/add-cancel")));
    expect(await claude.execute("/sdd.build", claudeRoot)).toMatchObject({
      ok: false,
      error: { code: "E_ARCHIVED_READONLY" },
    });
    expect(await codex.execute("sdd build", codexRoot)).toMatchObject({
      ok: false,
      error: { code: "E_ARCHIVED_READONLY" },
    });
  });

  it("keeps both hosts in CLARIFYING for blocker requirements", async () => {
    const claudeRoot = await fixture("blocker-claude");
    const codexRoot = await fixture("blocker-codex");
    const claude = new ClaudeCodeAdapter(core());
    const codex = new CodexAdapter(core());

    await claude.execute("/sdd.init", claudeRoot);
    await codex.execute("sdd init", codexRoot);

    expect(
      await claude.execute(
        `/sdd.new "${roughRequirement}" --change add-cancel`,
        claudeRoot,
      ),
    ).toMatchObject({
      ok: true,
      state: "CLARIFYING",
      next: "sdd new",
    });
    expect(
      await codex.execute(
        `sdd new "${roughRequirement}" --change add-cancel`,
        codexRoot,
      ),
    ).toMatchObject({
      ok: true,
      state: "CLARIFYING",
      next: "sdd new",
    });
  });
});
