import { access, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";

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

  it("initializes every required directory, index artifact, config, state, and audit log", async () => {
    const root = await project();
    const core = new Core({ codebase: new CodebaseAdapter() });

    const result = await core.execute({ command: "init", cwd: root });

    expect(result).toMatchObject({
      ok: true,
      state: "INDEX_READY",
      exitCode: 0,
      next: "sdd new",
      warnings: [expect.stringContaining("degraded")],
    });
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
  });

  it("is idempotent, preserves user config, and repairs a missing index artifact", async () => {
    const { rm } = await import("node:fs/promises");
    const root = await project();
    const core = new Core({ codebase: new CodebaseAdapter() });
    await core.execute({ command: "init", cwd: root });
    await writeFile(
      join(root, ".sdd/config.yml"),
      "schemaVersion: 1.0.0\ncustom: keep\n",
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
});
