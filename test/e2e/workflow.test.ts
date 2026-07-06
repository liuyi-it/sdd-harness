import { execFileSync } from "node:child_process";
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
const requirement =
  "Implement authenticated order cancellation through an API endpoint with authorization, conflict errors, audit logging, and automated tests.";

async function fixture(name: string): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), `sdd-e2e-${name}-`));
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
    const artifacts = [
      "spec.md",
      "spec.delta.md",
      "spec.model.json",
      "tasks.md",
      "traceability.md",
      "verify-report.md",
      "verify-report.v1.2.json",
      "review-report.md",
      "review-report.v1.2.json",
      "archive-report.md",
    ];
    for (const artifact of artifacts) {
      const claudeArtifact = await readFile(
        join(claudeRoot, ".sdd/changes/add-cancel", artifact),
        "utf8",
      );
      const codexArtifact = await readFile(
        join(codexRoot, ".sdd/changes/add-cancel", artifact),
        "utf8",
      );
      if (artifact.endsWith(".json")) {
        const normalize = (value: string) => {
          const parsed = JSON.parse(value) as Record<string, unknown>;
          delete parsed.generatedAt;
          return parsed;
        };
        expect(normalize(codexArtifact)).toEqual(normalize(claudeArtifact));
      } else {
        expect(codexArtifact).toBe(claudeArtifact);
      }
    }
    const specDelta = await readFile(
      join(claudeRoot, ".sdd/changes/add-cancel/spec.delta.md"),
      "utf8",
    );
    const tasks = await readFile(
      join(claudeRoot, ".sdd/changes/add-cancel/tasks.md"),
      "utf8",
    );
    const traceability = await readFile(
      join(claudeRoot, ".sdd/changes/add-cancel/traceability.md"),
      "utf8",
    );
    expect(specDelta).toContain("## ADDED Requirements");
    expect(tasks).toContain("Phase: RED");
    expect(tasks).toContain("Phase: VERIFY");
    expect(traceability).toContain("REQ-001-SC-001");
    expect(traceability).toContain("RED 命令：npm test");
    expect(traceability).toContain("最终验证命令：npm test");
    await expect(
      access(join(claudeRoot, ".sdd/index/mcp-capabilities.json")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(claudeRoot, ".sdd/index/codebase-diagnostics.json")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(claudeRoot, ".sdd/loop/loop.json")),
    ).resolves.toBeUndefined();
    const [loopRunFile] = await readdir(join(claudeRoot, ".sdd/loop/runs"));
    expect(loopRunFile).toBeDefined();
    const loopRun = JSON.parse(
      await readFile(join(claudeRoot, ".sdd/loop/runs", loopRunFile!), "utf8"),
    ) as {
      status: string;
      steps: Array<{ command: string; status: string }>;
    };
    expect(loopRun.status).toBe("ARCHIVED");
    expect(loopRun.steps.length).toBeGreaterThan(0);
    expect(loopRun.steps.map((step) => step.command)).toEqual(
      expect.arrayContaining([
        "new",
        "design",
        "plan",
        "build",
        "verify",
        "review",
        "archive",
      ]),
    );
    const [runId] = await readdir(join(claudeRoot, ".sdd/runs"));
    const taskArtifacts = await readdir(
      join(claudeRoot, ".sdd/runs", runId!, "tasks"),
    );
    expect(taskArtifacts.some((name) => name.endsWith(".result.json"))).toBe(
      true,
    );
  }, 30_000);

  it("keeps malicious repository text from changing the workflow and emits bounded context", async () => {
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
    expect(
      await adapter.execute(
        `sdd new "${requirement}" --change safe-change`,
        root,
      ),
    ).toMatchObject({ ok: true, state: "SPEC_READY" });
    expect(await adapter.execute("sdd design", root)).toMatchObject({
      ok: true,
      state: "DESIGN_READY",
    });
    expect(await adapter.execute("sdd plan", root)).toMatchObject({
      ok: true,
      state: "PLAN_READY",
    });

    const impact = await readFile(
      join(root, ".sdd/changes/safe-change/impact.md"),
      "utf8",
    );
    const contextPack = await readFile(
      join(root, ".sdd/context-packs/safe-change/TASK-001-RED.md"),
      "utf8",
    );
    expect(impact).toContain("UNTRUSTED_MCP_OUTPUT_BEGIN");
    expect(impact).toContain("README.md");
    expect(contextPack).toContain("## Security Rules");
    expect(contextPack).toContain("UNTRUSTED_REPOSITORY_CONTENT_BEGIN");
    expect(contextPack).toContain("UNTRUSTED_MCP_OUTPUT_BEGIN");
  });

  it("blocks review when a secret leaks into the worktree and redacts the report", async () => {
    const root = await fixture("secret-leak");
    const adapter = new CodexAdapter(core());
    await adapter.execute("sdd init", root);
    await adapter.execute(`sdd new "${requirement}" --change add-cancel`, root);
    await adapter.execute("sdd design", root);
    await adapter.execute("sdd plan", root);
    await adapter.execute("sdd build", root);
    await adapter.execute("sdd verify", root);

    await writeFile(
      join(root, "src/order.ts"),
      "export const order = {};\nexport const token = 'ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789';\n",
      "utf8",
    );

    const result = await adapter.execute("sdd review", root);
    const report = await readFile(
      join(root, ".sdd/changes/add-cancel/review-report.v1.2.md"),
      "utf8",
    );

    expect(result).toMatchObject({
      ok: false,
      error: { code: "E_REVIEW_FAILED" },
    });
    expect(report).toContain("SECRET_LEAK");
    expect(report).not.toContain("ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789");
  });
});
