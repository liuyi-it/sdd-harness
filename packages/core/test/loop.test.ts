import { access, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { CodebaseAdapter } from "../src/codebase/codebase-adapter.js";
import { Core } from "../src/core.js";
import { createInitialState, StateStore } from "../src/state/state-store.js";

const roots: string[] = [];

async function initializedProject(): Promise<{ root: string; core: Core }> {
  const root = await mkdtemp(join(tmpdir(), "sdd-loop-"));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Loop\n", "utf8");
  await writeFile(join(root, "package.json"), '{"name":"loop"}\n', "utf8");
  const core = new Core({ codebase: new CodebaseAdapter() });
  return { root, core };
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("loop store", () => {
  it("writes a default loop spec during init and overwrites on subsequent init", async () => {
    const { root, core } = await initializedProject();

    expect(await core.execute({ command: "init", cwd: root })).toMatchObject({
      ok: true,
      state: "INDEX_READY",
    });
    await expect(
      access(join(root, ".sdd/loop/loop.json")),
    ).resolves.toBeUndefined();
    expect(
      JSON.parse(await readFile(join(root, ".sdd/loop/loop.json"), "utf8")),
    ).toMatchObject({
      schemaVersion: "1.2.0",
      loopId: "auto-default",
      mode: "auto",
    });

    await writeFile(
      join(root, ".sdd/loop/loop.json"),
      '{ "schemaVersion": "1.2.0", "manual": true }\n',
      "utf8",
    );

    expect(await core.execute({ command: "init", cwd: root })).toMatchObject({
      ok: true,
      state: "INDEX_READY",
    });
    const overwritten = JSON.parse(
      await readFile(join(root, ".sdd/loop/loop.json"), "utf8"),
    );
    expect(overwritten).toMatchObject({
      schemaVersion: "1.2.0",
      loopId: "auto-default",
      mode: "auto",
    });
  });

  it("recovers activeLoop when the referenced run record is missing", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-loop-state-"));
    roots.push(root);
    const store = new StateStore(root);
    await mkdir(join(root, ".sdd", "loop"), { recursive: true });
    await store.write({
      ...createInitialState(),
      initialized: true,
      currentPhase: "INDEX_READY",
      activeLoop: {
        loopId: "auto-default",
        runId: "run-1",
        status: "RUNNING",
      },
    });

    const recovered = await store.read();

    expect(recovered.activeLoop).toMatchObject({
      loopId: "auto-default",
      runId: "run-1",
      status: "PAUSED",
      recovered: true,
    });
  });
});
