import { access, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";

// 初始化测试覆盖首次建仓、幂等修复以及生成文件与版本元数据的契约。
const roots: string[] = [];

async function project(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "sdd-init-"));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Fixture\n", "utf8");
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
        expect.stringContaining("config.yml 包含未知字段"),
      ]),
    );
    for (const path of [
      ".sdd/config.yml",
      ".sdd/state.json",
      ".sdd/index/codebase-summary.md",
      ".sdd/index/package-structure.md",
      ".sdd/index/architecture.md",
      ".sdd/logs/audit.log",
      ".sdd/changes",
      ".sdd/context-packs",
      ".sdd/runs",
      ".sdd/plugins",
      ".sdd/adapters",
      ".sdd/schemas/config.schema.json",
      ".sdd/schemas/state.schema.json",
      "CLAUDE.md",
      "AGENTS.md",
      ".claude/commands/sdd.init.md",
      ".claude/commands/sdd.status.md",
      ".claude/skills/sdd-harness/SKILL.md",
      ".codex/skills/sdd-harness/SKILL.md",
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
    });
    expect(
      await readFile(join(root, ".sdd/schemas/config.schema.json"), "utf8"),
    ).toBe(
      await readFile(join(process.cwd(), "schemas/config.schema.json"), "utf8"),
    );
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
      ".codex/skills/sdd-harness/SKILL.md",
    ]) {
      const content = await readFile(join(root, path), "utf8");
      expect(content).toContain("Think Before Coding");
      expect(content).toContain("Simplicity First");
      expect(content).toContain("Surgical Changes");
      expect(content).toContain("Goal-Driven Execution");
    }
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
    expect(await readFile(join(root, ".sdd/config.yml"), "utf8")).toContain(
      "custom: keep",
    );
    await expect(
      access(join(root, ".sdd/index/architecture.md")),
    ).resolves.toBeUndefined();
  });

  it("writes candidate files instead of overwriting manually edited integration files", async () => {
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
    expect(result.warnings).toEqual(
      expect.arrayContaining([
        expect.stringContaining("已生成候选文件供人工合并"),
      ]),
    );
    expect(
      await readFile(join(root, ".claude/commands/sdd.init.md"), "utf8"),
    ).toContain("manual change");
    await expect(
      access(join(root, ".claude/commands/sdd.init.md.candidate.md")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(root, "CLAUDE.md.candidate.md")),
    ).resolves.toBeUndefined();
  });

  it("keeps pre-existing unmanaged instruction files unchanged and writes candidates", async () => {
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
    expect(result.warnings).toEqual(
      expect.arrayContaining([
        expect.stringContaining("CLAUDE.md.candidate.md"),
        expect.stringContaining("AGENTS.md.candidate.md"),
      ]),
    );
    expect(await readFile(join(root, "CLAUDE.md"), "utf8")).toBe(
      "# Existing guide\n",
    );
    expect(await readFile(join(root, "AGENTS.md"), "utf8")).toBe(
      "# Existing agents\n",
    );
    expect(
      await readFile(join(root, "CLAUDE.md.candidate.md"), "utf8"),
    ).toContain("# Existing guide");
    expect(
      await readFile(join(root, "CLAUDE.md.candidate.md"), "utf8"),
    ).toContain("<!-- sdd-harness:managed -->");
    expect(
      await readFile(join(root, "AGENTS.md.candidate.md"), "utf8"),
    ).toContain("# Existing agents");
    expect(
      await readFile(join(root, "AGENTS.md.candidate.md"), "utf8"),
    ).toContain("<!-- sdd-harness:managed -->");
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

  it("fails init when config.yml misses required fields", async () => {
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
      ok: false,
      state: "FAILED",
      error: {
        code: "E_STATE_CORRUPTED",
        next: "sdd init",
      },
    });
    expect(result.error?.message).toContain("config.yml 校验失败");
  });
});
