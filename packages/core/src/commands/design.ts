import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import type { TddEngine } from "../engines/tdd/tdd-engine.js";
import { SddError } from "../errors.js";
import { FileLock } from "../state/file-lock.js";
import { StateStore } from "../state/state-store.js";

export async function runDesign(
  root: string,
  engine: TddEngine,
): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd design");
  try {
    const store = new StateStore(root);
    const state = await store.read();
    if (
      state.currentPhase !== "SPEC_READY" &&
      state.currentPhase !== "DESIGN_READY"
    ) {
      throw new SddError(
        "E_INVALID_PHASE_COMMAND",
        `Cannot design from ${state.currentPhase}`,
        state.suggestedCommand ?? undefined,
      );
    }
    const changeId = requireChangeId(state.currentChangeId);
    const change = join(root, ".sdd", "changes", changeId);
    const input = {
      spec: await readFile(join(change, "spec.md"), "utf8"),
      impact: await readFile(join(change, "impact.md"), "utf8"),
      codebaseSummary: await readFile(
        join(root, ".sdd/index/codebase-summary.md"),
        "utf8",
      ),
      packageStructure: await readFile(
        join(root, ".sdd/index/package-structure.md"),
        "utf8",
      ),
      architecture: await readFile(
        join(root, ".sdd/index/architecture.md"),
        "utf8",
      ),
    };
    const writer = new ArtifactWriter();
    const outcome = await writer.writeOrCandidate(
      join(change, "design.md"),
      engine.generateDesign(input),
      input,
    );
    if (outcome === "unchanged") {
      return {
        ok: true,
        state: "DESIGN_READY",
        exitCode: 0,
        changeId,
        next: "sdd plan",
        data: { alreadyReady: true },
      };
    }
    if (outcome === "candidate") {
      return {
        ok: true,
        state: "DESIGN_READY",
        exitCode: 0,
        changeId,
        next: "sdd plan",
        warnings: [
          "design input changed; generated design.md.candidate.md for manual merge",
        ],
      };
    }
    await store.update((current) => ({
      ...current,
      currentPhase: "DESIGNING",
      inProgressPhase: "DESIGNING",
      lastCommand: "sdd design",
    }));
    const ready = await store.update((current) => ({
      ...current,
      currentPhase: "DESIGN_READY",
      inProgressPhase: null,
      artifacts: { ...current.artifacts, design: "READY" },
      suggestedCommand: "sdd plan",
    }));
    await new AuditLogger(root).write({
      command: "sdd design",
      phase: ready.currentPhase,
      result: "PASS",
      changeId,
    });
    return {
      ok: true,
      state: ready.currentPhase,
      exitCode: 0,
      changeId,
      next: "sdd plan",
    };
  } finally {
    await lock.release();
  }
}

function requireChangeId(value: string | null): string {
  if (value === null)
    throw new SddError("E_MISSING_CHANGE", "No active change");
  return value;
}
