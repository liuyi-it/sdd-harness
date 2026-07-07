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
import { GitIsolationManager } from "../../packages/core/src/git-isolation/manager.js";
import { StateStore } from "../../packages/core/src/state/state-store.js";
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

async function attachWorkspace(root: string, changeId: string) {
  const manager = new GitIsolationManager(root, {
    createBranch: true,
    createWorktree: true,
    branchPattern: "sdd/<change-id>",
    worktreeDir: ".sdd/worktrees",
  });
  const workspace = await manager.ensure(changeId);
  const store = new StateStore(root);
  await store.write({
    ...(await store.read()),
    workspace: {
      branchName: workspace.branchName,
      worktreePath: workspace.worktreePath,
      baselineCommit: workspace.baselineCommit,
    },
  });
  return workspace;
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
      } else if (artifact === "archive-report.md") {
        const normalize = (value: string) =>
          value.replace(
            /- finalHead: [a-f0-9]{40}/g,
            "- finalHead: <normalized-head>",
          );
        expect(normalize(codexArtifact)).toBe(normalize(claudeArtifact));
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

  it("produces equivalent workspace metadata through Claude Code and Codex worktree workflows", async () => {
    const claudeRoot = await fixture("worktree-claude");
    const codexRoot = await fixture("worktree-codex");
    const claude = new ClaudeCodeAdapter(core());
    const codex = new CodexAdapter(core());

    await claude.execute("/sdd.init", claudeRoot);
    await codex.execute("sdd init", codexRoot);
    await claude.execute(
      `/sdd.new "${requirement}" --change add-cancel`,
      claudeRoot,
    );
    await codex.execute(
      `sdd new "${requirement}" --change add-cancel`,
      codexRoot,
    );
    await claude.execute("/sdd.design", claudeRoot);
    await codex.execute("sdd design", codexRoot);
    await claude.execute("/sdd.plan", claudeRoot);
    await codex.execute("sdd plan", codexRoot);

    const claudeWorkspace = await attachWorkspace(claudeRoot, "add-cancel");
    const codexWorkspace = await attachWorkspace(codexRoot, "add-cancel");

    for (const [command, expected] of [
      ["/sdd.build", "BUILD_READY"],
      ["/sdd.verify", "VERIFY_READY"],
      ["/sdd.review", "REVIEW_READY"],
      ["/sdd.archive", "ARCHIVED"],
    ] as const) {
      expect(await claude.execute(command, claudeRoot)).toMatchObject({
        ok: true,
        state: expected,
      });
    }
    for (const [command, expected] of [
      ["sdd build", "BUILD_READY"],
      ["sdd verify", "VERIFY_READY"],
      ["sdd review", "REVIEW_READY"],
      ["sdd archive", "ARCHIVED"],
    ] as const) {
      expect(await codex.execute(command, codexRoot)).toMatchObject({
        ok: true,
        state: expected,
      });
    }

    const claudeState = JSON.parse(
      await readFile(join(claudeRoot, ".sdd/state.json"), "utf8"),
    ) as { workspace?: Record<string, unknown> };
    const codexState = JSON.parse(
      await readFile(join(codexRoot, ".sdd/state.json"), "utf8"),
    ) as { workspace?: Record<string, unknown> };
    expect(claudeState.workspace).toMatchObject({
      branchName: claudeWorkspace.branchName,
      baselineCommit: claudeWorkspace.baselineCommit,
    });
    expect(codexState.workspace).toMatchObject({
      branchName: codexWorkspace.branchName,
      baselineCommit: codexWorkspace.baselineCommit,
    });

    const claudeArchive = await readFile(
      join(claudeRoot, ".sdd/changes/add-cancel/archive-report.md"),
      "utf8",
    );
    const codexArchive = await readFile(
      join(codexRoot, ".sdd/changes/add-cancel/archive-report.md"),
      "utf8",
    );
    expect(claudeArchive).toContain(
      `branchName: ${claudeWorkspace.branchName}`,
    );
    expect(codexArchive).toContain(`branchName: ${codexWorkspace.branchName}`);
    expect(claudeArchive).toContain("## 隔离工作区");
    expect(codexArchive).toContain("## 隔离工作区");
  }, 30_000);

  it("can resume a persisted worktree workflow with a fresh adapter instance", async () => {
    const root = await fixture("worktree-resume");
    const first = new CodexAdapter(core());
    await first.execute("sdd init", root);
    await first.execute(`sdd new "${requirement}" --change add-cancel`, root);
    await first.execute("sdd design", root);
    await first.execute("sdd plan", root);
    const workspace = await attachWorkspace(root, "add-cancel");

    expect(await first.execute("sdd build", root)).toMatchObject({
      ok: true,
      state: "BUILD_READY",
    });

    const second = new CodexAdapter(core());
    expect(await second.execute("sdd verify", root)).toMatchObject({
      ok: true,
      state: "VERIFY_READY",
    });
    expect(await second.execute("sdd review", root)).toMatchObject({
      ok: true,
      state: "REVIEW_READY",
    });
    expect(await second.execute("sdd archive", root)).toMatchObject({
      ok: true,
      state: "ARCHIVED",
    });

    const state = JSON.parse(
      await readFile(join(root, ".sdd/state.json"), "utf8"),
    ) as { workspace?: Record<string, unknown> };
    const report = await readFile(
      join(root, ".sdd/changes/add-cancel/archive-report.md"),
      "utf8",
    );
    expect(state.workspace).toMatchObject({
      branchName: workspace.branchName,
      baselineCommit: workspace.baselineCommit,
    });
    expect(report).toContain(`branchName: ${workspace.branchName}`);
  }, 30_000);

  it("blocks re-attaching the same change when the worktree is already dirty", async () => {
    const root = await fixture("worktree-conflict");
    const adapter = new ClaudeCodeAdapter(core());
    await adapter.execute("/sdd.init", root);
    await adapter.execute(
      `/sdd.new "${requirement}" --change add-cancel`,
      root,
    );
    await adapter.execute("/sdd.design", root);
    await adapter.execute("/sdd.plan", root);

    const workspace = await attachWorkspace(root, "add-cancel");
    await writeFile(
      join(workspace.businessRoot, "README.md"),
      "# dirty\n",
      "utf8",
    );

    await expect(attachWorkspace(root, "add-cancel")).rejects.toThrowError(
      expect.objectContaining({ code: "E_CONCURRENT_RUN" }),
    );
  });
});
