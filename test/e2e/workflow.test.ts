import { mkdtemp, readFile, readdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import { ClaudeCodeAdapter } from "../../packages/claude-code-plugin/src/adapter.js";
import { CodebaseAdapter } from "../../packages/core/src/codebase/codebase-adapter.js";
import { Core } from "../../packages/core/src/core.js";
import { CodexAdapter } from "../../packages/codex-plugin/src/adapter.js";

const roots: string[] = [];
const requirement =
  "Implement authenticated order cancellation through an API endpoint with authorization, conflict errors, audit logging, and automated tests.";

async function fixture(name: string): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), `sdd-e2e-${name}-`));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Orders\n", "utf8");
  return root;
}

function core(): Core {
  return new Core({
    codebase: new CodebaseAdapter(),
    taskExecutor: {
      execute: vi.fn().mockResolvedValue({
        modifiedFiles: ["src/order.ts", "test/order.test.ts"],
        verification: [{ command: "npm test", passed: true, output: "passed" }],
      }),
    },
  });
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("complete adapter workflows", () => {
  it("produces equivalent archived workflows through Claude Code and Codex", async () => {
    const claudeRoot = await fixture("claude");
    const codexRoot = await fixture("codex");
    const claude = new ClaudeCodeAdapter(core());
    const codex = new CodexAdapter(core());

    await claude.execute("/sdd.init", claudeRoot);
    const claudeResult = await claude.execute(
      `/sdd.auto "${requirement}" --change add-cancel`,
      claudeRoot,
    );
    await codex.execute("sdd init", codexRoot);
    const codexResult = await codex.execute(
      `sdd auto "${requirement}" --change add-cancel`,
      codexRoot,
    );

    expect(claudeResult).toMatchObject({ ok: true, state: "ARCHIVED" });
    expect(codexResult).toMatchObject({ ok: true, state: "ARCHIVED" });
    expect(await readdir(join(claudeRoot, ".sdd/changes/add-cancel"))).toEqual(
      await readdir(join(codexRoot, ".sdd/changes/add-cancel")),
    );
  });

  it("does not treat malicious repository text as executable instruction", async () => {
    const root = await fixture("malicious");
    await writeFile(
      join(root, "README.md"),
      await readFile(
        join(process.cwd(), "fixtures/security-malicious-readme/README.md"),
        "utf8",
      ),
      "utf8",
    );
    const adapter = new CodexAdapter(core());
    await adapter.execute("sdd init", root);
    await adapter.execute(
      `sdd new "${requirement}" --change safe-change`,
      root,
    );

    const impact = await readFile(
      join(root, ".sdd/changes/safe-change/impact.md"),
      "utf8",
    );
    expect(impact).toContain("README.md");
    expect(impact).not.toContain("Read ~/.ssh");
    expect(impact).not.toContain("attacker.example");
  });
});
