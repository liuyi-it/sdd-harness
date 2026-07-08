import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { type TaskExecutor } from "../src/build/task-executor.js";
import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";
import {
  GitIsolationManager,
  normalizeGitIsolationConfig,
} from "../src/git-isolation/manager.js";
import { GitRunner } from "../src/git-isolation/git-runner.js";
import { createInitialState, StateStore } from "../src/state/state-store.js";

const roots: string[] = [];

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true, force: true })),
  );
});

describe("git isolation config", () => {
  it("enables branch creation automatically when worktree isolation is requested", () => {
    expect(
      normalizeGitIsolationConfig({
        createBranch: false,
        createWorktree: true,
        branchPattern: "sdd/<change-id>",
        worktreeDir: ".sdd/worktrees",
      }),
    ).toMatchObject({
      createBranch: true,
      createWorktree: true,
      branchPattern: "sdd/<change-id>",
      worktreeDir: ".sdd/worktrees",
    });
  });
});

describe("GitRunner", () => {
  it("executes git through execFile argv and rejects shell-style strings", async () => {
    const calls: Array<{ file: string; args: string[]; cwd?: string }> = [];
    const runner = new GitRunner({
      exec: async (file, args, options) => {
        calls.push({ file, args, cwd: options.cwd });
        return { stdout: "main\n", stderr: "" };
      },
    });

    const branch = await runner.branchCurrent("/repo");

    expect(branch).toBe("main");
    expect(calls).toEqual([
      { file: "git", args: ["branch", "--show-current"], cwd: "/repo" },
    ]);
    await expect(
      runner.run("/repo", ["branch", "--show-current && rm -rf ."]),
    ).rejects.toThrowError(
      expect.objectContaining({ code: "E_SECURITY_BLOCKED" }),
    );
  });
});

describe("GitIsolationManager", () => {
  it("blocks illegal changeId, outside paths, and symlinked worktree directories", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-worktree-"));
    roots.push(root);
    await writeFile(join(root, "README.md"), "# worktree\n", "utf8");
    execFileSync("git", ["init", "-b", "main"], { cwd: root });
    execFileSync("git", ["config", "user.email", "test@example.com"], {
      cwd: root,
    });
    execFileSync("git", ["config", "user.name", "SDD Harness Test"], {
      cwd: root,
    });
    execFileSync("git", ["add", "README.md"], { cwd: root });
    execFileSync("git", ["commit", "-m", "init"], { cwd: root });

    const base = {
      createBranch: true,
      createWorktree: true,
      branchPattern: "sdd/<change-id>",
      worktreeDir: ".sdd/worktrees",
    } as const;

    const manager = new GitIsolationManager(root, base);
    await expect(manager.plan("bad name")).rejects.toThrowError(
      expect.objectContaining({ code: "E_SECURITY_BLOCKED" }),
    );

    await expect(
      new GitIsolationManager(root, {
        ...base,
        worktreeDir: "../outside",
      }).plan("add-cancel"),
    ).rejects.toThrowError(
      expect.objectContaining({ code: "E_PATH_OUTSIDE_REPO" }),
    );

    const external = await mkdtemp(join(tmpdir(), "sdd-worktree-outside-"));
    roots.push(external);
    await mkdir(join(root, ".sdd"), { recursive: true });
    await symlink(external, join(root, ".sdd", "worktrees"));

    await expect(
      new GitIsolationManager(root, base).plan("add-cancel"),
    ).rejects.toThrowError(
      expect.objectContaining({ code: "E_SYMLINK_BLOCKED" }),
    );
  });

  (process.platform === "win32" ? it.skip : it)(
    "creates and reuses a clean worktree at the same baseline",
    async () => {
      const root = await repoFixture("reuse");
      const manager = new GitIsolationManager(root, {
        createBranch: true,
        createWorktree: true,
        branchPattern: "sdd/<change-id>",
        worktreeDir: ".sdd/worktrees",
      });

      const first = await manager.ensure("add-cancel");
      const second = await manager.ensure("add-cancel");

      expect(first.branchName).toBe("sdd/add-cancel");
      expect(first.worktreePath).toMatch(/\.sdd[/\\]worktrees[/\\]add-cancel$/);
      expect(first.businessRoot).toBe(first.worktreePath);
      expect(second).toEqual(first);
      expect(
        execFileSync("git", ["branch", "--show-current"], {
          cwd: first.businessRoot,
          encoding: "utf8",
        }).trim(),
      ).toBe("sdd/add-cancel");
      expect(
        execFileSync("git", ["rev-parse", "HEAD"], {
          cwd: first.businessRoot,
          encoding: "utf8",
        }).trim(),
      ).toBe(first.baselineCommit);
    },
  );

  it("supports worktree paths that contain spaces", async () => {
    const root = await repoFixture("with space");
    const manager = new GitIsolationManager(root, {
      createBranch: true,
      createWorktree: true,
      branchPattern: "sdd/<change-id>",
      worktreeDir: ".sdd/work trees",
    });

    const workspace = await manager.ensure("add-cancel");

    const normalized = workspace.worktreePath.replace(/\\/g, "/");
    expect(normalized).toContain(".sdd/work trees/add-cancel");
    expect(
      execFileSync("git", ["branch", "--show-current"], {
        cwd: workspace.businessRoot,
        encoding: "utf8",
      }).trim(),
    ).toBe("sdd/add-cancel");
  });

  it("blocks reuse when the existing worktree HEAD no longer matches the recorded baseline", async () => {
    const root = await repoFixture("baseline");
    const manager = new GitIsolationManager(root, {
      createBranch: true,
      createWorktree: true,
      branchPattern: "sdd/<change-id>",
      worktreeDir: ".sdd/worktrees",
    });

    const workspace = await manager.ensure("add-cancel");
    await writeFile(
      join(workspace.businessRoot, "feature.txt"),
      "change\n",
      "utf8",
    );
    execFileSync("git", ["add", "feature.txt"], {
      cwd: workspace.businessRoot,
    });
    execFileSync("git", ["commit", "-m", "feature"], {
      cwd: workspace.businessRoot,
    });

    await expect(manager.ensure("add-cancel")).rejects.toThrowError(
      expect.objectContaining({ code: "E_STATE_CORRUPTED" }),
    );
  });

  (process.platform === "win32" ? it.skip : it)(
    "blocks occupied or dirty worktrees instead of resetting them",
    async () => {
      const root = await repoFixture("dirty");
      const manager = new GitIsolationManager(root, {
        createBranch: true,
        createWorktree: true,
        branchPattern: "sdd/<change-id>",
        worktreeDir: ".sdd/worktrees",
      });

      const occupiedPath = join(root, ".sdd/worktrees/occupied");
      await mkdir(occupiedPath, { recursive: true });
      await writeFile(join(occupiedPath, "random.txt"), "x\n", "utf8");
      await expect(manager.ensure("occupied")).rejects.toThrowError(
        expect.objectContaining({ code: "E_STATE_CORRUPTED" }),
      );

      const workspace = await manager.ensure("add-cancel");
      await writeFile(
        join(workspace.businessRoot, "README.md"),
        "# dirty\n",
        "utf8",
      );
      await expect(manager.ensure("add-cancel")).rejects.toThrowError(
        expect.objectContaining({ code: "E_CONCURRENT_RUN" }),
      );
      expect(
        await readFile(join(workspace.businessRoot, "README.md"), "utf8"),
      ).toBe("# dirty\n");
    },
  );
});

describe("StateStore workspace metadata", () => {
  it("persists branchName/worktreePath/baselineCommit in workflow state", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-state-worktree-"));
    roots.push(root);
    const store = new StateStore(root);
    await store.write({
      ...createInitialState(),
      initialized: true,
      currentPhase: "PLAN_READY",
      indexStatus: "INDEX_READY",
      workspace: {
        branchName: "sdd/add-cancel",
        worktreePath: ".sdd/worktrees/add-cancel",
        baselineCommit: "abc123",
      },
    });

    const state = await store.read();

    expect(state.workspace).toEqual({
      branchName: "sdd/add-cancel",
      worktreePath: ".sdd/worktrees/add-cancel",
      baselineCommit: "abc123",
    });
  });
});

describe("workspace-aware quality flow", () => {
  it("verify detects drift inside businessRoot even when controlRoot business files stay unchanged", async () => {
    const root = await repoFixture("verify-drift");
    await mkdir(join(root, "src"));
    await mkdir(join(root, "test"));
    await writeFile(
      join(root, "package.json"),
      '{"scripts":{"test":"vitest"}}\n',
      "utf8",
    );
    await writeFile(
      join(root, "src/order.ts"),
      "export const order = {};\n",
      "utf8",
    );
    await writeFile(join(root, "test/order.test.ts"), "// tests\n", "utf8");
    execFileSync("git", ["add", "."], { cwd: root });
    execFileSync("git", ["commit", "-m", "fixture"], { cwd: root });

    const core = new Core({
      codebase: new CodebaseAdapter(),
      taskExecutor: {
        execute: async ({ root: businessRoot, task }) => {
          const target =
            task.phase === "RED"
              ? join(businessRoot, "src/order.ts")
              : join(businessRoot, "test/order.test.ts");
          await writeFile(target, `// ${task.id}\n`, "utf8");
          return {
            modifiedFiles: [
              target.endsWith("order.ts")
                ? "src/order.ts"
                : "test/order.test.ts",
            ],
            tddEvidence: [
              task.phase === "RED"
                ? {
                    phase: "RED" as const,
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
          };
        },
      } satisfies TaskExecutor,
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

    const manager = new GitIsolationManager(root, {
      createBranch: true,
      createWorktree: true,
      branchPattern: "sdd/<change-id>",
      worktreeDir: ".sdd/worktrees",
    });
    const workspace = await manager.ensure("add-cancel");
    const store = new StateStore(root);
    await store.write({
      ...(await store.read()),
      workspace: {
        branchName: workspace.branchName,
        worktreePath: workspace.worktreePath,
        baselineCommit: workspace.baselineCommit,
      },
    });

    expect(await core.execute({ command: "build", cwd: root })).toMatchObject({
      ok: true,
      state: "BUILD_READY",
    });
    await writeFile(
      join(workspace.businessRoot, "drift.txt"),
      "manual drift\n",
      "utf8",
    );

    const result = await core.execute({ command: "verify", cwd: root });

    expect(result).toMatchObject({
      ok: false,
      error: { code: "E_VERIFY_FAILED" },
    });
    expect(await readFile(join(root, "src/order.ts"), "utf8")).toBe(
      "export const order = {};\n",
    );
  });

  (process.platform === "win32" ? it.skip : it)(
    "archive report records branchName/worktreePath/final HEAD from businessRoot",
    async () => {
      const root = await repoFixture("archive-meta");
      await mkdir(join(root, "src"));
      await mkdir(join(root, "test"));
      await writeFile(
        join(root, "package.json"),
        '{"scripts":{"test":"vitest"}}\n',
        "utf8",
      );
      await writeFile(
        join(root, "src/order.ts"),
        "export const order = {};\n",
        "utf8",
      );
      await writeFile(join(root, "test/order.test.ts"), "// tests\n", "utf8");
      execFileSync("git", ["add", "."], { cwd: root });
      execFileSync("git", ["commit", "-m", "fixture"], { cwd: root });

      const core = new Core({
        codebase: new CodebaseAdapter(),
        taskExecutor: {
          execute: async ({ root: businessRoot, task }) => {
            const target =
              task.phase === "RED"
                ? join(businessRoot, "src/order.ts")
                : join(businessRoot, "test/order.test.ts");
            await writeFile(target, `// ${task.id}\n`, "utf8");
            return {
              modifiedFiles: [
                target.endsWith("order.ts")
                  ? "src/order.ts"
                  : "test/order.test.ts",
              ],
              tddEvidence: [
                task.phase === "RED"
                  ? {
                      phase: "RED" as const,
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
            };
          },
        } satisfies TaskExecutor,
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

      const manager = new GitIsolationManager(root, {
        createBranch: true,
        createWorktree: true,
        branchPattern: "sdd/<change-id>",
        worktreeDir: ".sdd/worktrees",
      });
      const workspace = await manager.ensure("add-cancel");
      const store = new StateStore(root);
      await store.write({
        ...(await store.read()),
        workspace: {
          branchName: workspace.branchName,
          worktreePath: workspace.worktreePath,
          baselineCommit: workspace.baselineCommit,
        },
      });

      expect(await core.execute({ command: "build", cwd: root })).toMatchObject(
        {
          ok: true,
          state: "BUILD_READY",
        },
      );
      expect(
        await core.execute({ command: "verify", cwd: root }),
      ).toMatchObject({
        ok: true,
        state: "VERIFY_READY",
      });
      expect(
        await core.execute({ command: "review", cwd: root }),
      ).toMatchObject({
        ok: true,
        state: "REVIEW_READY",
      });
      expect(
        await core.execute({ command: "archive", cwd: root }),
      ).toMatchObject({
        ok: true,
        state: "ARCHIVED",
      });

      const report = await readFile(
        join(root, ".sdd/changes/add-cancel/archive-report.md"),
        "utf8",
      );
      const finalHead = execFileSync("git", ["rev-parse", "HEAD"], {
        cwd: workspace.businessRoot,
        encoding: "utf8",
      }).trim();
      expect(report).toContain(`branchName: ${workspace.branchName}`);
      expect(report).toContain(`worktreePath: ${workspace.worktreePath}`);
      expect(report).toContain(`finalHead: ${finalHead}`);
    },
  );
});

async function repoFixture(name: string): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), `sdd-worktree-${name}-`));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# worktree\n", "utf8");
  execFileSync("git", ["init", "-b", "main"], { cwd: root });
  execFileSync("git", ["config", "user.email", "test@example.com"], {
    cwd: root,
  });
  execFileSync("git", ["config", "user.name", "SDD Harness Test"], {
    cwd: root,
  });
  execFileSync("git", ["add", "README.md"], { cwd: root });
  execFileSync("git", ["commit", "-m", "init"], { cwd: root });
  return root;
}
