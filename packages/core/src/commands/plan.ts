import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import type { TddEngine } from "../engines/tdd/tdd-engine.js";
import { SddError } from "../errors.js";
import { FileLock } from "../state/file-lock.js";
import { StateStore } from "../state/state-store.js";

export async function runPlan(
  root: string,
  engine: TddEngine,
): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd plan");
  try {
    const store = new StateStore(root);
    const state = await store.read();
    if (
      state.currentPhase !== "DESIGN_READY" &&
      state.currentPhase !== "PLAN_READY"
    ) {
      throw new SddError(
        "E_INVALID_PHASE_COMMAND",
        `Cannot plan from ${state.currentPhase}`,
        state.suggestedCommand ?? undefined,
      );
    }
    if (state.currentChangeId === null)
      throw new SddError("E_MISSING_CHANGE", "No active change");
    const changeId = state.currentChangeId;
    const change = join(root, ".sdd", "changes", changeId);
    const input = {
      spec: await readFile(join(change, "spec.md"), "utf8"),
      design: await readFile(join(change, "design.md"), "utf8"),
      impact: await readFile(join(change, "impact.md"), "utf8"),
      codebaseSummary: await readFile(
        join(root, ".sdd/index/codebase-summary.md"),
        "utf8",
      ),
    };
    const artifacts = engine.generatePlan(input);
    const writer = new ArtifactWriter();
    const outcomes = await Promise.all([
      writer.writeOrCandidate(
        join(change, "tasks.md"),
        artifacts.tasksMarkdown,
        input,
      ),
      writer.writeOrCandidate(
        join(change, "test-plan.md"),
        artifacts.testPlan,
        input,
      ),
      writer.writeOrCandidate(
        join(change, "context.md"),
        artifacts.context,
        input,
      ),
    ]);
    if (outcomes.every((outcome) => outcome === "unchanged")) {
      return {
        ok: true,
        state: "PLAN_READY",
        exitCode: 0,
        changeId,
        next: "sdd build",
        data: { alreadyReady: true },
      };
    }
    if (outcomes.some((outcome) => outcome === "candidate")) {
      return {
        ok: true,
        state: "PLAN_READY",
        exitCode: 0,
        changeId,
        next: "sdd build",
        warnings: [
          "plan input changed; generated candidate artifacts for manual merge",
        ],
      };
    }
    await store.update((current) => ({
      ...current,
      currentPhase: "PLANNING",
      inProgressPhase: "PLANNING",
      lastCommand: "sdd plan",
    }));
    await writeFile(
      join(change, "tasks.json"),
      `${JSON.stringify(artifacts.tasks, null, 2)}\n`,
      "utf8",
    );
    const packDirectory = join(root, ".sdd", "context-packs", changeId);
    await mkdir(packDirectory, { recursive: true });
    await Promise.all(
      Object.entries(artifacts.contextPacks).map(([taskId, content]) =>
        writer.write(
          join(packDirectory, `${taskId}.md`),
          contextPackWithMetadata(content, input),
          input,
        ),
      ),
    );
    const tasks = Object.fromEntries(
      artifacts.tasks.map((task) => [task.id, task.status]),
    );
    const ready = await store.update((current) => ({
      ...current,
      currentPhase: "PLAN_READY",
      inProgressPhase: null,
      tasks,
      artifacts: {
        ...current.artifacts,
        tasks: "READY",
        testPlan: "READY",
        context: "READY",
      },
      suggestedCommand: "sdd build",
    }));
    await new AuditLogger(root).write({
      command: "sdd plan",
      phase: ready.currentPhase,
      result: "PASS",
      changeId,
    });
    return {
      ok: true,
      state: ready.currentPhase,
      exitCode: 0,
      changeId,
      next: "sdd build",
    };
  } finally {
    await lock.release();
  }
}

function contextPackWithMetadata(
  content: string,
  input: {
    spec: string;
    design: string;
    impact: string;
    codebaseSummary: string;
  },
): string {
  const metadata = [
    "<!-- Context Pack Metadata",
    `Codebase Index Hash: ${hash(input.codebaseSummary)}`,
    `Source Artifact Hash: ${hash(`${input.spec}\n${input.design}\n${input.impact}`)}`,
    `Generated At: ${new Date().toISOString()}`,
    "-->",
    "",
  ].join("\n");
  return truncateUtf8(`${metadata}${content}`, 30 * 1024);
}

function hash(value: string): string {
  return `sha256:${createHash("sha256").update(value).digest("hex")}`;
}

function truncateUtf8(value: string, maxBytes: number): string {
  if (Buffer.byteLength(value) <= maxBytes) return value;
  let end = value.length;
  while (end > 0 && Buffer.byteLength(value.slice(0, end)) > maxBytes - 32)
    end -= 1;
  return `${value.slice(0, end)}\n\n[Context truncated]\n`;
}
