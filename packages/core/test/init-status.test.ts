import { execFileSync } from "node:child_process";
import { access, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";
import { sha256Hex } from "../src/dependency-integrity.js";

// 初始化测试覆盖首次建仓、幂等修复以及生成文件与版本元数据的契约。
const roots: string[] = [];

async function project(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "sdd-init-"));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Fixture\n", "utf8");
  await writeFile(join(root, "package.json"), '{"name":"fixture"}\n', "utf8");
  return root;
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("init and status", () => {
  it("reports NOT_INITIALIZED without mutating the repository", async () => {
    const root = await project();
    const core = new Core({ codebase: new CodebaseAdapter() });

    const result = await core.execute({ command: "status", cwd: root });

    expect(result).toMatchObject({
      ok: true,
      state: "NOT_INITIALIZED",
      exitCode: 0,
      next: "sdd init",
    });
    await expect(access(join(root, ".sdd"))).rejects.toThrow();
  });

  it("在 verbose 模式下返回调试信息", async () => {
    const root = await project();
    const core = new Core({ codebase: new CodebaseAdapter() });

    const result = await core.execute({
      command: "status",
      cwd: root,
      args: { verbose: true },
    });

    expect(result).toMatchObject({
      ok: true,
      state: "NOT_INITIALIZED",
      data: {
        debug: {
          command: "status",
          cwd: root,
          verbose: true,
        },
      },
    });
  });

  it("warns when status is recovered from backup or artifacts", async () => {
    const root = await project();
    await mkdir(join(root, ".sdd/index"), { recursive: true });
    await writeFile(join(root, ".sdd/state.json"), "invalid", "utf8");
    await writeFile(join(root, ".sdd/state.json.bak"), "invalid", "utf8");
    await writeFile(
      join(root, ".sdd/index/codebase-summary.md"),
      "summary",
      "utf8",
    );
    await writeFile(
      join(root, ".sdd/index/package-structure.md"),
      "pkg",
      "utf8",
    );
    await writeFile(join(root, ".sdd/index/architecture.md"), "arch", "utf8");

    const result = await new Core({ codebase: new CodebaseAdapter() }).execute({
      command: "status",
      cwd: root,
    });

    expect(result).toMatchObject({
      ok: true,
      state: "INDEX_READY",
      next: "sdd new",
    });
    expect(result.warnings).toEqual(
      expect.arrayContaining([
        expect.stringContaining("状态已从备份或制品恢复"),
      ]),
    );
  });

  it("warns when status detects pre-existing uncommitted changes", async () => {
    const root = await project();
    execFileSync("git", ["init", "-b", "main"], { cwd: root });
    execFileSync("git", ["config", "user.email", "test@example.com"], {
      cwd: root,
    });
    execFileSync("git", ["config", "user.name", "SDD Harness Test"], {
      cwd: root,
    });
    await writeFile(join(root, "README.md"), "# Repo\n", "utf8");
    execFileSync("git", ["add", "README.md"], { cwd: root });
    execFileSync("git", ["commit", "-m", "init"], { cwd: root });
    await writeFile(join(root, "README.md"), "# Repo changed\n", "utf8");

    const core = new Core({ codebase: new CodebaseAdapter() });
    await core.execute({ command: "init", cwd: root });
    const result = await core.execute({ command: "status", cwd: root });

    expect(result.warnings).toEqual(
      expect.arrayContaining([
        expect.stringContaining("检测到执行前已有未提交修改"),
      ]),
    );
  });

  it("status 会提示当前处于降级模式", async () => {
    const root = await project();
    const core = new Core({ codebase: new CodebaseAdapter() });

    await core.execute({ command: "init", cwd: root });
    const result = await core.execute({ command: "status", cwd: root });

    expect(result.warnings).toEqual(
      expect.arrayContaining([
        expect.stringContaining("降级模式"),
        expect.stringContaining("codebase-memory-mcp unavailable"),
      ]),
    );
  });

  it("pauses init for an empty project until structurePolicy is provided", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-init-empty-"));
    roots.push(root);
    await writeFile(join(root, "README.md"), "# Empty\n", "utf8");
    const core = new Core({ codebase: new CodebaseAdapter() });

    const first = await core.execute({ command: "init", cwd: root });

    expect(first).toMatchObject({
      ok: true,
      state: "CLARIFYING",
      next: "sdd init",
    });

    const second = await core.execute({
      command: "init",
      cwd: root,
      args: { structurePolicy: "free-design" },
    });

    expect(second).toMatchObject({
      ok: true,
      state: "INDEX_READY",
      next: "sdd new",
    });
    expect(
      JSON.parse(
        await readFile(join(root, ".sdd/project/conventions.json"), "utf8"),
      ),
    ).toMatchObject({
      schemaVersion: "1.2.0",
      projectType: "empty",
      strategy: "free-design",
    });
  });

  it("initializes every required directory, index artifact, config, state, and audit log", async () => {
    const root = await project();
    const core = new Core({ codebase: new CodebaseAdapter() });

    const result = await core.execute({ command: "init", cwd: root });

    expect(result).toMatchObject({
      ok: true,
      state: "INDEX_READY",
      exitCode: 0,
      next: "sdd new",
    });
    expect(result.warnings).toEqual(
      expect.arrayContaining([
        expect.stringContaining("降级模式"),
        expect.stringContaining("官方项目地址"),
        expect.stringContaining("config.yml 包含未知字段"),
      ]),
    );
    for (const path of [
      ".sdd/config.yml",
      ".sdd/state.json",
      ".sdd/index/codebase-summary.md",
      ".sdd/index/codebase-summary.md.meta.json",
      ".sdd/index/package-structure.md",
      ".sdd/index/package-structure.md.meta.json",
      ".sdd/index/architecture.md",
      ".sdd/index/architecture.md.meta.json",
      ".sdd/index/codebase-diagnostics.json",
      ".sdd/logs/audit.log",
      ".sdd/changes",
      ".sdd/context-packs",
      ".sdd/runs",
      ".sdd/plugins",
      ".sdd/adapters",
      ".sdd/adapters/codebase-memory-mcp/integrity.json",
      ".sdd/schemas/config.schema.json",
      ".sdd/schemas/state.schema.json",
      "CLAUDE.md",
      "CLAUDE.md.meta.json",
      "AGENTS.md",
      "AGENTS.md.meta.json",
      ".claude/commands/sdd.init.md",
      ".claude/commands/sdd.init.md.meta.json",
      ".claude/commands/sdd.status.md",
      ".claude/skills/sdd-harness/SKILL.md",
      ".claude/skills/sdd-harness/SKILL.md.meta.json",
      ".codex/commands/sdd.init.md",
      ".codex/commands/sdd.init.md.meta.json",
      ".codex/commands/sdd.status.md",
      ".codex/skills/sdd-harness/SKILL.md",
      ".codex/skills/sdd-harness/SKILL.md.meta.json",
      ".opencode/commands/sdd.init.md",
      ".opencode/commands/sdd.init.md.meta.json",
      ".opencode/commands/sdd.status.md",
      ".opencode/skills/sdd-harness/SKILL.md",
      ".opencode/skills/sdd-harness/SKILL.md.meta.json",
    ]) {
      await expect(access(join(root, path))).resolves.toBeUndefined();
    }
    expect(
      JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
    ).toMatchObject({
      initialized: true,
      currentPhase: "INDEX_READY",
      degraded: true,
      codebaseProvider: "fallback-file-scan",
      degradedReason: "codebase-memory-mcp unavailable",
    });
    expect(await readFile(join(root, ".sdd/config.yml"), "utf8")).toContain(
      "fallbackProvider: file-scan",
    );
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/index/codebase-diagnostics.json"),
          "utf8",
        ),
      ),
    ).toMatchObject({
      installed: false,
      configured: false,
      connected: false,
      callable: false,
      indexed: false,
    });
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/index/codebase-summary.md.meta.json"),
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
    expect(
      await readFile(join(root, ".sdd/schemas/config.schema.json"), "utf8"),
    ).toBe(
      await readFile(join(process.cwd(), "schemas/config.schema.json"), "utf8"),
    );
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/adapters/codebase-memory-mcp/integrity.json"),
          "utf8",
        ),
      ),
    ).toMatchObject({
      component: "codebase-memory-mcp",
      status: "skipped",
    });
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/adapters/codebase-memory-mcp/version.json"),
          "utf8",
        ),
      ),
    ).toMatchObject({
      name: "codebase-memory-mcp",
      version: "v0.8.1",
      commit: "f0c9be19c5d74b84f418d807bfdce7b5d6a261ff",
      interface: "mcp",
      status: "unavailable",
      checksumManifest:
        "https://github.com/DeusData/codebase-memory-mcp/releases/download/v0.8.1/checksums.txt",
      checksumManifestSha256:
        "142399e4e552fb559ede866b2549dbacc942d56f1c8718b52bc701b21f3f94c6",
    });
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/adapters/openspec/version.json"),
          "utf8",
        ),
      ),
    ).toMatchObject({
      name: "openspec",
      version: "v1.4.1",
      commit: "1b06fddd59d8e592d5b5794a1970b22867e85b1f",
      license: "MIT",
      interface: "vendored-module",
    });
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/adapters/superpowers/version.json"),
          "utf8",
        ),
      ),
    ).toMatchObject({
      name: "superpowers",
      version: "v6.1.1",
      commit: "d884ae04edebef577e82ff7c4e143debd0bbec99",
      license: "MIT",
      interface: "vendored-module",
    });
    for (const path of [
      "CLAUDE.md",
      "AGENTS.md",
      ".claude/commands/sdd.init.md",
      ".claude/skills/sdd-harness/SKILL.md",
      ".codex/commands/sdd.init.md",
      ".codex/skills/sdd-harness/SKILL.md",
      ".opencode/commands/sdd.init.md",
      ".opencode/skills/sdd-harness/SKILL.md",
    ]) {
      const content = await readFile(join(root, path), "utf8");
      expect(content).toContain("Think Before Coding");
      expect(content).toContain("Simplicity First");
      expect(content).toContain("Surgical Changes");
      expect(content).toContain("Goal-Driven Execution");
    }
  });

  it("写命令在锁等待超时时返回 E_LOCK_TIMEOUT", async () => {
    const root = await project();
    await mkdir(join(root, ".sdd"), { recursive: true });
    const { FileLock } = await import("../src/state/file-lock.js");
    const lock = new FileLock(root);
    await lock.acquire("sdd build");

    const core = new Core({ codebase: new CodebaseAdapter() });
    const result = await core.execute({
      command: "init",
      cwd: root,
      args: { timeout: 0.01 },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "NOT_INITIALIZED",
      exitCode: 9,
      error: {
        code: "E_LOCK_TIMEOUT",
      },
    });

    await lock.release();
  });

  it("init 在超时后进入 FAILED 并记录恢复上下文", async () => {
    const root = await project();
    const core = new Core({
      codebase: new CodebaseAdapter({
        isAvailable: async () => true,
        index: async () => {
          await new Promise((resolve) => setTimeout(resolve, 50));
        },
        summarize: async () => ({
          codebaseSummary: "summary",
          packageStructure: "pkg",
          architecture: "arch",
        }),
      }),
    });

    const result = await core.execute({
      command: "init",
      cwd: root,
      args: { timeout: 0.01 },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      exitCode: 124,
      error: { code: "E_TIMEOUT", next: "sdd init" },
    });
    expect(
      JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
    ).toMatchObject({
      currentPhase: "FAILED",
      previousPhase: "NOT_INITIALIZED",
      inProgressPhase: "INDEXING",
      failedCommand: "sdd init",
      suggestedCommand: "sdd init",
      lastError: "E_TIMEOUT",
    });
  });

  it("提供完整性校验材料时会在 init 过程中完成校验并落盘报告", async () => {
    const root = await project();
    const artifact = Buffer.from("archive-bytes");
    const artifactSha256 = sha256Hex(artifact);
    const manifestContent = `${artifactSha256}  codebase-memory-mcp-darwin-arm64.tar.gz\n`;
    const core = new Core({ codebase: new CodebaseAdapter() });

    const result = await core.execute({
      command: "init",
      cwd: root,
      args: {
        integrity: {
          codebaseMemoryMcp: {
            manifestContent,
            expectedManifestSha256: sha256Hex(manifestContent),
            artifactName: "codebase-memory-mcp-darwin-arm64.tar.gz",
            artifactContentBase64: artifact.toString("base64"),
          },
        },
      },
    });

    expect(result).toMatchObject({
      ok: true,
      state: "INDEX_READY",
    });
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/adapters/codebase-memory-mcp/integrity.json"),
          "utf8",
        ),
      ),
    ).toMatchObject({
      component: "codebase-memory-mcp",
      status: "verified",
      artifactName: "codebase-memory-mcp-darwin-arm64.tar.gz",
      verifiedAt: expect.any(String),
    });
  });

  it("完整性校验失败时 init 返回 E_COMPONENT_INTEGRITY_FAILED", async () => {
    const root = await project();
    const core = new Core({ codebase: new CodebaseAdapter() });

    const result = await core.execute({
      command: "init",
      cwd: root,
      args: {
        integrity: {
          codebaseMemoryMcp: {
            manifestContent:
              "deadbeef  codebase-memory-mcp-darwin-arm64.tar.gz\n",
            artifactName: "codebase-memory-mcp-darwin-arm64.tar.gz",
            artifactContentBase64:
              Buffer.from("archive-bytes").toString("base64"),
          },
        },
      },
    });

    expect(result).toMatchObject({
      ok: false,
      state: "FAILED",
      exitCode: 10,
      error: { code: "E_COMPONENT_INTEGRITY_FAILED" },
    });
  });

  it("init 在收到中断后进入 PAUSED 并记录恢复上下文", async () => {
    const root = await project();
    const controller = new AbortController();
    const core = new Core({
      codebase: new CodebaseAdapter({
        isAvailable: async () => true,
        index: async () => {
          await new Promise((resolve) => setTimeout(resolve, 50));
        },
        summarize: async () => ({
          codebaseSummary: "summary",
          packageStructure: "pkg",
          architecture: "arch",
        }),
      }),
    });
    setTimeout(() => controller.abort(), 10);

    const result = await core.execute({
      command: "init",
      cwd: root,
      signal: controller.signal,
    });

    expect(result).toMatchObject({
      ok: false,
      state: "PAUSED",
      exitCode: 130,
      error: { code: "E_INTERRUPTED", next: "sdd init" },
    });
    expect(
      JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
    ).toMatchObject({
      currentPhase: "PAUSED",
      interruptedCommand: "sdd init",
      previousPhase: "NOT_INITIALIZED",
      inProgressPhase: "INDEXING",
      suggestedCommand: "sdd init",
      lastError: "E_INTERRUPTED",
    });
  });

  it("init 超时失败后再次执行可以恢复成功", async () => {
    const root = await project();
    let attempts = 0;
    const core = new Core({
      codebase: new CodebaseAdapter({
        isAvailable: async () => true,
        index: async () => {
          attempts += 1;
          if (attempts === 1) {
            await new Promise((resolve) => setTimeout(resolve, 50));
          }
        },
        summarize: async () => ({
          codebaseSummary: "summary",
          packageStructure: "pkg",
          architecture: "arch",
        }),
      }),
    });

    expect(
      await core.execute({
        command: "init",
        cwd: root,
        args: { timeout: 0.01 },
      }),
    ).toMatchObject({
      ok: false,
      state: "FAILED",
      error: { code: "E_TIMEOUT" },
    });

    expect(await core.execute({ command: "init", cwd: root })).toMatchObject({
      ok: true,
      state: "INDEX_READY",
      next: "sdd new",
    });
  });

  it("is idempotent, preserves user config, and repairs a missing index artifact", async () => {
    const { rm } = await import("node:fs/promises");
    const root = await project();
    const core = new Core({ codebase: new CodebaseAdapter() });
    await core.execute({ command: "init", cwd: root });
    await writeFile(
      join(root, ".sdd/config.yml"),
      [
        'schemaVersion: "1.0.0"',
        "project:",
        "  name: fixture",
        "plugins:",
        "  claudeCode:",
        "    enabled: true",
        "  codex:",
        "    enabled: true",
        "codebase:",
        '  provider: "codebase-memory-mcp"',
        '  fallbackProvider: "file-scan"',
        "  autoIndexOnInit: true",
        "workflow:",
        "  maxClarifyingQuestionsPerRound: 5",
        "  requireBlockerAnswers: true",
        "  stopOnFailure: true",
        "quality:",
        "  requireFileScopeCheck: true",
        "  requireDriftCheck: true",
        "security:",
        "  blockOutsideRepo: true",
        "  blockSymlinksOutsideRepo: true",
        "  redactSecretsInLogs: true",
        "custom: keep",
        "",
      ].join("\n"),
      "utf8",
    );
    await rm(join(root, ".sdd/index/architecture.md"));

    const result = await core.execute({ command: "init", cwd: root });

    expect(result.ok).toBe(true);
    await expect(
      access(join(root, ".sdd/index/architecture.md")),
    ).resolves.toBeUndefined();
  });

  it("migrates legacy 1.0.0 state and config during init", async () => {
    const root = await project();
    await mkdir(join(root, ".sdd"), { recursive: true });
    await writeFile(
      join(root, ".sdd/state.json"),
      `${JSON.stringify({
        schemaVersion: "1.0.0",
        version: 4,
        updatedAt: new Date().toISOString(),
        initialized: true,
        currentChangeId: null,
        currentRunId: null,
        currentPhase: "INDEX_READY",
        indexStatus: "INDEX_READY",
        codebaseProvider: "codebase-memory-mcp",
        degraded: false,
        degradedReason: null,
        lastCommand: "sdd init",
        lastError: null,
        previousPhase: null,
        inProgressPhase: null,
        failedCommand: null,
        failedReason: null,
        interruptedCommand: null,
        recoverable: true,
        suggestedCommand: "sdd new",
        tasks: {},
        artifacts: {},
      })}\n`,
      "utf8",
    );
    await writeFile(
      join(root, ".sdd/config.yml"),
      [
        'schemaVersion: "1.0.0"',
        "project:",
        "  name: fixture",
        "plugins:",
        "  claudeCode:",
        "    enabled: true",
        "  codex:",
        "    enabled: true",
        "codebase:",
        '  provider: "codebase-memory-mcp"',
        '  fallbackProvider: "file-scan"',
        "  autoIndexOnInit: true",
        "workflow:",
        "  maxClarifyingQuestionsPerRound: 5",
        "  requireBlockerAnswers: true",
        "  stopOnFailure: true",
        "quality:",
        "  requireFileScopeCheck: true",
        "  requireDriftCheck: true",
        "security:",
        "  blockOutsideRepo: true",
        "  blockSymlinksOutsideRepo: true",
        "  redactSecretsInLogs: true",
        "",
      ].join("\n"),
      "utf8",
    );

    const result = await new Core({ codebase: new CodebaseAdapter() }).execute({
      command: "init",
      cwd: root,
    });

    expect(result).toMatchObject({
      ok: true,
      state: "INDEX_READY",
    });
    expect(
      JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
    ).toMatchObject({
      schemaVersion: "1.2.0",
      activeLoop: null,
    });
    expect(await readFile(join(root, ".sdd/config.yml"), "utf8")).toContain(
      "schemaVersion: 1.2.0",
    );
    expect(
      await readFile(join(root, ".sdd/migration-report.md"), "utf8"),
    ).toContain("目标 schemaVersion：1.2.0");
  });

  it("writes integration files with line-dedup merge for manually edited files", async () => {
    const root = await project();
    const core = new Core({ codebase: new CodebaseAdapter() });
    await core.execute({ command: "init", cwd: root });
    await writeFile(
      join(root, ".claude/commands/sdd.init.md"),
      "---\ndescription: customized\n---\n\nmanual change\n",
      "utf8",
    );
    await writeFile(
      join(root, "CLAUDE.md"),
      "<!-- sdd-harness:managed -->\nmanual override\n",
      "utf8",
    );

    const result = await core.execute({ command: "init", cwd: root });

    expect(result).toMatchObject({
      ok: true,
      state: "INDEX_READY",
    });
    // 指令文件采用行级去重追加，保留原有 "manual override" 行并追加缺失的受管行
    // 命令文件会被直接覆盖（属于受管文件）
    expect(
      await readFile(join(root, ".claude/commands/sdd.init.md"), "utf8"),
    ).toContain("通过 sdd-harness 执行 sdd init");
    const claudeContent = await readFile(join(root, "CLAUDE.md"), "utf8");
    expect(claudeContent).toContain("manual override");
    expect(claudeContent).toContain("## sdd-harness");
  });

  it("传入 --force 时直接覆盖受管集成文件而不是生成 candidate", async () => {
    const root = await project();
    const core = new Core({ codebase: new CodebaseAdapter() });
    await core.execute({ command: "init", cwd: root });
    await writeFile(
      join(root, ".claude/commands/sdd.init.md"),
      "---\ndescription: customized\n---\n\nmanual change\n",
      "utf8",
    );
    await writeFile(
      join(root, "CLAUDE.md"),
      "<!-- sdd-harness:managed -->\nmanual override\n",
      "utf8",
    );

    const result = await core.execute({
      command: "init",
      cwd: root,
      args: { force: true },
    });

    expect(result).toMatchObject({
      ok: true,
      state: "INDEX_READY",
    });
    expect(
      await readFile(join(root, ".claude/commands/sdd.init.md"), "utf8"),
    ).toContain("通过 sdd-harness 执行 sdd init");
    expect(await readFile(join(root, "CLAUDE.md"), "utf8")).toContain(
      "<!-- sdd-harness:managed -->",
    );
    await expect(
      access(join(root, ".claude/commands/sdd.init.md.candidate.md")),
    ).rejects.toThrow();
    await expect(
      access(join(root, "CLAUDE.md.candidate.md")),
    ).rejects.toThrow();
  });

  it("appends managed content to pre-existing instruction files via line dedup", async () => {
    const root = await project();
    await writeFile(join(root, "CLAUDE.md"), "# Existing guide\n", "utf8");
    await writeFile(join(root, "AGENTS.md"), "# Existing agents\n", "utf8");

    const result = await new Core({ codebase: new CodebaseAdapter() }).execute({
      command: "init",
      cwd: root,
    });

    expect(result).toMatchObject({
      ok: true,
      state: "INDEX_READY",
    });
    // 指令文件采用行级去重追加：原有行保持不变，受管内容追加到末尾
    const claudeContent = await readFile(join(root, "CLAUDE.md"), "utf8");
    expect(claudeContent).toContain("# Existing guide");
    expect(claudeContent).toContain("<!-- sdd-harness:managed -->");
    expect(claudeContent).toContain("## sdd-harness");
    // 原有行不应重复
    const guideLines = claudeContent
      .split("\n")
      .filter((l) => l === "# Existing guide");
    expect(guideLines.length).toBe(1);

    const agentsContent = await readFile(join(root, "AGENTS.md"), "utf8");
    expect(agentsContent).toContain("# Existing agents");
    expect(agentsContent).toContain("<!-- sdd-harness:managed -->");
    expect(agentsContent).toContain("## sdd-harness");
    const agentsLines = agentsContent
      .split("\n")
      .filter((l) => l === "# Existing agents");
    expect(agentsLines.length).toBe(1);
  });

  it("validates existing config and warns on unknown keys", async () => {
    const root = await project();
    const core = new Core({ codebase: new CodebaseAdapter() });
    await core.execute({ command: "init", cwd: root });
    await writeFile(
      join(root, ".sdd/config.yml"),
      [
        'schemaVersion: "1.0.0"',
        "project:",
        "  name: custom",
        "plugins:",
        "  claudeCode:",
        "    enabled: true",
        "codebase:",
        '  provider: "codebase-memory-mcp"',
        "workflow:",
        "  maxClarifyingQuestionsPerRound: 5",
        "quality:",
        "  requireFileScopeCheck: true",
        "security:",
        "  blockOutsideRepo: true",
        "customFlag: true",
        "",
      ].join("\n"),
      "utf8",
    );

    const result = await core.execute({ command: "init", cwd: root });

    expect(result).toMatchObject({ ok: true, state: "INDEX_READY" });
    expect(result.warnings).toEqual(
      expect.arrayContaining([
        expect.stringContaining("config.yml 包含未知字段"),
      ]),
    );
  });

  it("recovers from invalid config.yml by overwriting with default config", async () => {
    const root = await project();
    const core = new Core({ codebase: new CodebaseAdapter() });
    await core.execute({ command: "init", cwd: root });
    await writeFile(
      join(root, ".sdd/config.yml"),
      ['schemaVersion: "1.0.0"', "project:", "  name: invalid", ""].join("\n"),
      "utf8",
    );

    const result = await core.execute({ command: "init", cwd: root });

    expect(result).toMatchObject({
      ok: true,
      state: "INDEX_READY",
    });
  });
});
