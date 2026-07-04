import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { AuditLogger } from "../src/audit/audit-logger.js";
import { FileLock } from "../src/state/file-lock.js";
import { createInitialState, StateStore } from "../src/state/state-store.js";

// 状态层测试覆盖原子写入、迁移、损坏恢复和文件锁回收等底层保障能力。
const roots: string[] = [];

async function temporaryRoot(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "sdd-state-"));
  roots.push(root);
  return root;
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("StateStore", () => {
  it("writes and updates state atomically", async () => {
    const root = await temporaryRoot();
    const store = new StateStore(root);

    await store.write(createInitialState());
    await store.update((state) => ({ ...state, currentPhase: "INDEX_READY" }));

    expect((await store.read()).currentPhase).toBe("INDEX_READY");
    expect(
      JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
    ).toMatchObject({
      schemaVersion: "1.0.0",
      version: 2,
      updatedAt: expect.any(String),
    });
    expect(await new AuditLogger(root).entries()).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          command: "state.write",
          phase: "NOT_INITIALIZED",
          result: "PASS",
          message: "state version 1",
        }),
        expect.objectContaining({
          command: "state.write",
          phase: "INDEX_READY",
          result: "PASS",
          message: "state version 2",
        }),
      ]),
    );
  });

  it("recovers a corrupted state file from its backup", async () => {
    const root = await temporaryRoot();
    const store = new StateStore(root);
    await store.write(createInitialState());
    await store.update((state) => ({ ...state, currentPhase: "INDEX_READY" }));
    await writeFile(join(root, ".sdd/state.json"), "not-json", "utf8");

    const recovered = await store.read();

    expect(recovered.currentPhase).toBe("NOT_INITIALIZED");
    expect(recovered.recoveredFromBackup).toBe(true);
  });

  it("migrates a version 0 state and preserves a migration backup", async () => {
    const root = await temporaryRoot();
    const store = new StateStore(root);
    await store.write(createInitialState());
    await mkdir(join(root, ".sdd", "changes", "legacy-change"), {
      recursive: true,
    });
    await writeFile(
      join(root, ".sdd", "changes", "legacy-change", "spec.md"),
      "# Legacy spec\n",
      "utf8",
    );
    const legacy = {
      ...createInitialState(),
      schemaVersion: "0.9.0",
      version: 7,
    };
    await writeFile(
      join(root, ".sdd/state.json"),
      `${JSON.stringify(legacy)}\n`,
      "utf8",
    );

    const migrated = await store.read();

    expect(migrated.schemaVersion).toBe("1.0.0");
    expect(migrated.version).toBe(8);
    expect(
      JSON.parse(
        await readFile(join(root, ".sdd/state.json.migration.bak"), "utf8"),
      ),
    ).toMatchObject({ schemaVersion: "0.9.0", version: 7 });
    expect(
      await readFile(join(root, ".sdd/logs/migration.log"), "utf8"),
    ).toContain("0.9.0 -> 1.0.0");
    expect(
      await readFile(join(root, ".sdd/migration-report.md"), "utf8"),
    ).toContain("目标 schemaVersion：1.0.0");
    expect(
      await readFile(join(root, ".sdd/migration-report.md"), "utf8"),
    ).toContain(".sdd/state.json.migration.bak");
    expect(
      await readFile(join(root, ".sdd/migration-report.md"), "utf8"),
    ).toContain(".sdd.migration.bak");
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/migration-report.md.meta.json"),
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
    await expect(
      readFile(
        join(root, ".sdd.migration.bak", "changes", "legacy-change", "spec.md"),
        "utf8",
      ),
    ).resolves.toContain("# Legacy spec");
  });

  it("infers and writes a recovered state when state and backup are invalid", async () => {
    const root = await temporaryRoot();
    await mkdir(join(root, ".sdd/index"), { recursive: true });
    await writeFile(
      join(root, ".sdd/index/codebase-summary.md"),
      "summary",
      "utf8",
    );
    await mkdir(join(root, ".sdd/changes/add-cancel"), { recursive: true });
    await writeFile(
      join(root, ".sdd/changes/add-cancel/spec.md"),
      "# Spec",
      "utf8",
    );
    await writeFile(join(root, ".sdd/state.json"), "invalid", "utf8");
    await writeFile(join(root, ".sdd/state.json.bak"), "invalid", "utf8");

    const recovered = await new StateStore(root).read();

    expect(recovered).toMatchObject({
      initialized: true,
      currentChangeId: "add-cancel",
      currentPhase: "SPEC_READY",
      recoveredFromBackup: true,
    });
    await expect(
      readFile(join(root, ".sdd/state.recovered.json"), "utf8"),
    ).resolves.toContain("SPEC_READY");
  });

  it("normalizes an orphaned in-progress state to FAILED with recovery context", async () => {
    const root = await temporaryRoot();
    const store = new StateStore(root);
    await store.write({
      ...createInitialState(),
      initialized: true,
      currentChangeId: "add-cancel",
      currentPhase: "BUILDING",
      previousPhase: "PLAN_READY",
      inProgressPhase: "BUILDING",
      lastCommand: "sdd build",
      suggestedCommand: "sdd build",
      tasks: {
        "TASK-001": "DONE",
        "TASK-002": "BUILDING",
      },
    });
    await mkdir(join(root, ".sdd", "changes", "add-cancel"), {
      recursive: true,
    });

    const normalized = await store.read();

    expect(normalized).toMatchObject({
      currentPhase: "FAILED",
      previousPhase: "PLAN_READY",
      inProgressPhase: "BUILDING",
      failedCommand: "sdd build",
      suggestedCommand: "sdd build",
      recoverable: true,
      lastError: "E_INTERRUPTED",
      tasks: {
        "TASK-001": "DONE",
        "TASK-002": "FAILED",
      },
    });
    expect(normalized.failedReason).toContain("已自动规范化为 FAILED");
    expect(
      JSON.parse(await readFile(join(root, ".sdd/state.json"), "utf8")),
    ).toMatchObject({
      currentPhase: "FAILED",
      failedCommand: "sdd build",
      inProgressPhase: "BUILDING",
    });
  });
});

describe("FileLock", () => {
  it("rejects a second writer while a live lock exists", async () => {
    const root = await temporaryRoot();
    const first = new FileLock(root);
    const second = new FileLock(root);
    await first.acquire("sdd build");

    await expect(second.acquire("sdd plan")).rejects.toMatchObject({
      code: "E_CONCURRENT_RUN",
    });

    await first.release();
    await expect(second.acquire("sdd plan")).resolves.toBeUndefined();
    await second.release();
  });

  it("records command ownership and expiry metadata", async () => {
    const root = await temporaryRoot();
    const lock = new FileLock(root);
    await lock.acquire("sdd build", "add-cancel");

    const metadata = JSON.parse(
      await readFile(join(root, ".sdd/lock"), "utf8"),
    );

    expect(metadata).toMatchObject({
      pid: process.pid,
      command: "sdd build",
      changeId: "add-cancel",
      createdAt: expect.any(String),
      expiresAt: expect.any(String),
    });
    expect(Date.parse(metadata.expiresAt)).toBeGreaterThan(
      Date.parse(metadata.createdAt),
    );
    await lock.release();
  });

  it("replaces a stale lock whose process no longer exists", async () => {
    const root = await temporaryRoot();
    await mkdir(join(root, ".sdd"), { recursive: true });
    await writeFile(
      join(root, ".sdd/lock"),
      JSON.stringify({
        pid: 2_147_483_647,
        command: "sdd build",
        startedAt: "2000-01-01",
      }),
      "utf8",
    );
    const lock = new FileLock(root);

    await expect(lock.acquire("sdd plan")).resolves.toBeUndefined();
    await lock.release();
  });

  it("当锁在等待窗口内始终被占用时返回 E_LOCK_TIMEOUT", async () => {
    const root = await temporaryRoot();
    const first = new FileLock(root);
    const second = new FileLock(root);
    await first.acquire("sdd build");

    await expect(
      second.acquire("sdd plan", undefined, { timeoutMs: 20, retryDelayMs: 5 }),
    ).rejects.toMatchObject({
      code: "E_LOCK_TIMEOUT",
      exitCode: 9,
    });

    await first.release();
  });
});
