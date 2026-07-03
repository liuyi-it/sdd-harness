import {
  appendFile,
  copyFile,
  mkdir,
  open,
  readFile,
  readdir,
  rename,
  stat,
  writeFile,
} from "node:fs/promises";
import { dirname, join } from "node:path";

import { z } from "zod";

import { PHASES, type Phase } from "../contracts.js";
import { SddError } from "../errors.js";

const taskStatusSchema = z.enum([
  "PENDING",
  "BUILDING",
  "DONE",
  "FAILED",
  "SKIPPED",
]);

export const workflowStateSchema = z.object({
  schemaVersion: z.literal("1.0.0"),
  version: z.number().int().positive(),
  updatedAt: z.string(),
  initialized: z.boolean(),
  currentChangeId: z.string().nullable(),
  currentRunId: z.string().nullable(),
  currentPhase: z.enum(PHASES),
  indexStatus: z.enum([
    "MISSING",
    "INDEXING",
    "INDEX_READY",
    "STALE",
    "UNAVAILABLE",
  ]),
  codebaseProvider: z.string(),
  degraded: z.boolean(),
  degradedReason: z.string().nullable(),
  lastCommand: z.string().nullable(),
  lastError: z.string().nullable(),
  previousPhase: z.enum(PHASES).nullable(),
  inProgressPhase: z.enum(PHASES).nullable(),
  failedCommand: z.string().nullable(),
  interruptedCommand: z.string().nullable(),
  suggestedCommand: z.string().nullable(),
  tasks: z.record(z.string(), taskStatusSchema),
  artifacts: z.record(
    z.string(),
    z.enum(["MISSING", "READY", "CANDIDATE", "STALE"]),
  ),
  recoveredFromBackup: z.boolean().optional(),
});

export type WorkflowState = z.infer<typeof workflowStateSchema>;

export function createInitialState(): WorkflowState {
  return {
    schemaVersion: "1.0.0",
    version: 1,
    updatedAt: new Date().toISOString(),
    initialized: false,
    currentChangeId: null,
    currentRunId: null,
    currentPhase: "NOT_INITIALIZED",
    indexStatus: "MISSING",
    codebaseProvider: "codebase-memory-mcp",
    degraded: false,
    degradedReason: null,
    lastCommand: null,
    lastError: null,
    previousPhase: null,
    inProgressPhase: null,
    failedCommand: null,
    interruptedCommand: null,
    suggestedCommand: "sdd init",
    tasks: {},
    artifacts: {},
  };
}

export class StateStore {
  readonly path: string;
  readonly backupPath: string;

  constructor(readonly root: string) {
    this.path = join(root, ".sdd", "state.json");
    this.backupPath = `${this.path}.bak`;
  }

  async read(): Promise<WorkflowState> {
    try {
      const raw = JSON.parse(await readFile(this.path, "utf8")) as Record<
        string,
        unknown
      >;
      if (raw.schemaVersion === "0.9.0") {
        const migrated = workflowStateSchema.parse({
          ...raw,
          schemaVersion: "1.0.0",
          version: typeof raw.version === "number" ? raw.version + 1 : 1,
          updatedAt: new Date().toISOString(),
        });
        await writeFile(
          `${this.path}.migration.bak`,
          `${JSON.stringify(raw, null, 2)}\n`,
          "utf8",
        );
        await mkdir(join(this.root, ".sdd", "logs"), { recursive: true });
        await appendFile(
          join(this.root, ".sdd", "logs", "migration.log"),
          `${new Date().toISOString()} 0.9.0 -> 1.0.0\n`,
          "utf8",
        );
        await this.write(migrated);
        return migrated;
      }
      const parsed = workflowStateSchema.parse(raw);
      await this.validateChangeReference(parsed);
      return parsed;
    } catch (error) {
      try {
        const backup = workflowStateSchema.parse(
          JSON.parse(await readFile(this.backupPath, "utf8")),
        );
        return { ...backup, recoveredFromBackup: true };
      } catch {
        const inferred = await this.recoverFromArtifacts();
        if (inferred !== null) return inferred;
        throw new SddError(
          "E_STATE_CORRUPTED",
          `Unable to read state or backup: ${error instanceof Error ? error.message : String(error)}`,
          "sdd status",
        );
      }
    }
  }

  async write(state: WorkflowState): Promise<void> {
    const validated = workflowStateSchema.parse(state);
    await mkdir(dirname(this.path), { recursive: true });
    const temporaryPath = `${this.path}.${process.pid}.${Date.now()}.tmp`;
    try {
      await copyFile(this.path, this.backupPath);
    } catch (error) {
      if (!isMissingFile(error)) throw error;
    }
    const handle = await open(temporaryPath, "w");
    await handle.writeFile(`${JSON.stringify(validated, null, 2)}\n`, "utf8");
    await handle.sync();
    await handle.close();
    await rename(temporaryPath, this.path);
    try {
      const directory = await open(dirname(this.path), "r");
      await directory.sync();
      await directory.close();
    } catch {
      // Directory fsync is not supported by every Windows filesystem.
    }
  }

  async update(
    updater: (state: WorkflowState) => WorkflowState,
  ): Promise<WorkflowState> {
    const current = await this.read();
    const updated = workflowStateSchema.parse({
      ...updater(current),
      version: current.version + 1,
      updatedAt: new Date().toISOString(),
    });
    await this.write(updated);
    return updated;
  }

  private async validateChangeReference(state: WorkflowState): Promise<void> {
    if (state.currentChangeId === null) return;
    try {
      if (
        !(
          await stat(join(this.root, ".sdd", "changes", state.currentChangeId))
        ).isDirectory()
      ) {
        throw new Error("not a directory");
      }
    } catch {
      throw new SddError(
        "E_STATE_CORRUPTED",
        `currentChangeId does not exist: ${state.currentChangeId}`,
        "sdd status",
      );
    }
  }

  private async recoverFromArtifacts(): Promise<WorkflowState | null> {
    const indexPath = join(this.root, ".sdd", "index", "codebase-summary.md");
    if (!(await pathExists(indexPath))) return null;
    let changeIds: string[] = [];
    try {
      changeIds = (await readdir(join(this.root, ".sdd", "changes"))).sort();
    } catch {
      // No changes means the index is ready.
    }
    const changeId = changeIds.at(-1) ?? null;
    let phase: Phase = "INDEX_READY";
    if (changeId !== null) {
      const change = join(this.root, ".sdd", "changes", changeId);
      if (await pathExists(join(change, ".archived"))) phase = "ARCHIVED";
      else if (await reportPassed(join(change, "review-report.md")))
        phase = "REVIEW_READY";
      else if (await reportPassed(join(change, "verify-report.md")))
        phase = "VERIFY_READY";
      else if (await allTasksDone(join(change, "task-results.json")))
        phase = "BUILD_READY";
      else if (await pathExists(join(change, "tasks.md"))) phase = "PLAN_READY";
      else if (await pathExists(join(change, "design.md")))
        phase = "DESIGN_READY";
      else if (await pathExists(join(change, "spec.md"))) phase = "SPEC_READY";
    }
    const recovered: WorkflowState = {
      ...createInitialState(),
      version: 1,
      initialized: true,
      currentChangeId: changeId,
      currentPhase: phase,
      indexStatus: "INDEX_READY",
      recoveredFromBackup: true,
      suggestedCommand: suggestedCommand(phase),
    };
    await writeFile(
      join(this.root, ".sdd", "state.recovered.json"),
      `${JSON.stringify(recovered, null, 2)}\n`,
      "utf8",
    );
    return recovered;
  }
}

function isMissingFile(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    error.code === "ENOENT"
  );
}

export function isPhase(value: string): value is Phase {
  return (PHASES as readonly string[]).includes(value);
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

async function reportPassed(path: string): Promise<boolean> {
  try {
    return (await readFile(path, "utf8")).includes("## Result\n\nPASS");
  } catch {
    return false;
  }
}

async function allTasksDone(path: string): Promise<boolean> {
  try {
    const results = JSON.parse(await readFile(path, "utf8")) as unknown[];
    return results.length > 0;
  } catch {
    return false;
  }
}

function suggestedCommand(phase: Phase): string | null {
  return (
    {
      INDEX_READY: "sdd new",
      SPEC_READY: "sdd design",
      DESIGN_READY: "sdd plan",
      PLAN_READY: "sdd build",
      BUILD_READY: "sdd verify",
      VERIFY_READY: "sdd review",
      REVIEW_READY: "sdd archive",
    }[phase as string] ?? null
  );
}
