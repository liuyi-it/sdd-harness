import {
  appendFile,
  cp,
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

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { readCompactPlan } from "../artifacts/change-artifacts.js";
import { PHASES, type Phase } from "../contracts.js";
import { SddError } from "../errors.js";
import { LoopStore } from "../loop/loop-store.js";
import {
  CURRENT_SCHEMA_VERSION,
  migrateWorkflowState,
} from "./schema-migration.js";

/**
 * StateStore 负责工作流状态的读取、校验、迁移、原子写入与恢复推断。
 * 它维护的是 `.sdd/` 事实源中最核心的一份状态文件。
 */
const taskStatusSchema = z.enum([
  "PENDING",
  "BUILDING",
  "DONE",
  "FAILED",
  "SKIPPED",
]);

const safeIdSchema = z.string().regex(/^[A-Za-z0-9][A-Za-z0-9_-]*$/);
const safeRelativePathSchema = z
  .string()
  .min(1)
  .refine(
    (value) =>
      !value.startsWith("/") &&
      !value.includes("\\") &&
      !value.split("/").includes(".."),
    "必须是安全相对路径",
  );

const pendingAgentTaskSchema = z.object({
  taskId: safeIdSchema,
  resultFile: safeRelativePathSchema,
  since: z.string().datetime(),
  gitBaseline: z.object({
    available: z.literal(true),
    files: z.array(safeRelativePathSchema),
    hashes: z.record(z.string(), z.string()),
    tracked: z.array(safeRelativePathSchema),
  }),
});

const activeLoopSchema = z
  .object({
    loopId: safeIdSchema,
    runId: safeIdSchema,
    status: z.enum([
      "RUNNING",
      "WAITING_AGENT",
      "PAUSED",
      "FAILED",
      "SUCCEEDED",
      "ABORTED",
      "ARCHIVED",
    ]),
    waiting: z
      .object({
        reason: z.enum([
          "AGENT_TASK_EXECUTION",
          "CLARIFICATION",
          "HUMAN_REVIEW",
        ]),
        taskId: safeIdSchema.optional(),
        resultFile: safeRelativePathSchema.optional(),
        since: z.string().datetime(),
      })
      .optional(),
    recovered: z.boolean().optional(),
    lastDecision: z.string().optional(),
  })
  .superRefine((loop, context) => {
    if (loop.status === "WAITING_AGENT" && loop.waiting === undefined)
      context.addIssue({
        code: "custom",
        message: "WAITING_AGENT 必须包含 waiting",
      });
    if (
      loop.waiting?.reason === "AGENT_TASK_EXECUTION" &&
      (loop.waiting.taskId === undefined ||
        loop.waiting.resultFile === undefined)
    )
      context.addIssue({
        code: "custom",
        message: "任务等待必须包含 taskId 和 resultFile",
      });
    if (
      ["ABORTED", "SUCCEEDED", "ARCHIVED"].includes(loop.status) &&
      loop.waiting !== undefined
    )
      context.addIssue({
        code: "custom",
        message: "终态 loop 不能保留 waiting",
      });
  });

export const workflowStateSchema = z
  .object({
    schemaVersion: z.literal(CURRENT_SCHEMA_VERSION),
    version: z.number().int().positive(),
    updatedAt: z.string(),
    initialized: z.boolean(),
    currentChangeId: z.string().nullable(),
    currentRunId: z.string().nullable(),
    activeLoop: activeLoopSchema.nullable(),
    pendingAgentTask: pendingAgentTaskSchema.nullable().default(null),
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
    failedReason: z.string().nullable().optional(),
    interruptedCommand: z.string().nullable(),
    recoverable: z.boolean().optional(),
    suggestedCommand: z.string().nullable(),
    workspace: z
      .object({
        branchName: z.string().nullable(),
        worktreePath: z.string().nullable(),
        baselineCommit: z.string(),
      })
      .nullable()
      .optional(),
    tasks: z.record(z.string(), taskStatusSchema),
    artifacts: z.record(
      z.string(),
      z.enum(["MISSING", "READY", "CANDIDATE", "STALE"]),
    ),
    recoveredFromBackup: z.boolean().optional(),
  })
  .superRefine((state, context) => {
    const pending = state.pendingAgentTask;
    if (pending !== null && state.tasks[pending.taskId] !== "BUILDING")
      context.addIssue({
        code: "custom",
        message: "pendingAgentTask 必须对应 BUILDING 任务",
      });
    const waiting = state.activeLoop?.waiting;
    if (
      waiting?.reason === "AGENT_TASK_EXECUTION" &&
      (state.pendingAgentTask === null ||
        waiting.taskId !== state.pendingAgentTask.taskId ||
        waiting.resultFile !== state.pendingAgentTask.resultFile)
    )
      context.addIssue({
        code: "custom",
        message: "activeLoop waiting 必须与 pendingAgentTask 保持一致",
      });
  });

export type WorkflowState = z.infer<typeof workflowStateSchema>;

export function createInitialState(): WorkflowState {
  return {
    schemaVersion: CURRENT_SCHEMA_VERSION,
    version: 1,
    updatedAt: new Date().toISOString(),
    initialized: false,
    currentChangeId: null,
    currentRunId: null,
    activeLoop: null,
    pendingAgentTask: null,
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
    failedReason: null,
    interruptedCommand: null,
    recoverable: true,
    suggestedCommand: "sdd init",
    workspace: null,
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
      if (raw.schemaVersion === "1.0.0") {
        const backupDirectory = await backupSddDirectory(this.root);
        const migration = migrateWorkflowState(raw);
        const migrated = workflowStateSchema.parse(migration.state);
        await writeFile(
          `${this.path}.migration.bak`,
          `${JSON.stringify(raw, null, 2)}\n`,
          "utf8",
        );
        await mkdir(join(this.root, ".sdd", "logs"), { recursive: true });
        await appendFile(
          join(this.root, ".sdd", "logs", "migration.log"),
          `${new Date().toISOString()} ${migration.from} -> ${migration.to}\n`,
          "utf8",
        );
        await new ArtifactWriter().write(
          join(this.root, ".sdd", "migration-report.md"),
          migrationReport({
            fromSchemaVersion: migration.from,
            toSchemaVersion: migration.to,
            nextVersion: migrated.version,
            backupPaths: migration.backupPaths,
            backupDirectory,
            ...(typeof raw.version === "number"
              ? { previousVersion: raw.version }
              : {}),
          }),
          {
            fromSchemaVersion: migration.from,
            toSchemaVersion: migration.to,
            nextVersion: migrated.version,
            backupPaths: migration.backupPaths,
            backupDirectory,
            ...(typeof raw.version === "number"
              ? { previousVersion: raw.version }
              : {}),
          },
        );
        await this.write(migrated);
        return migrated;
      }
      if (raw.schemaVersion === "1.2.0") {
        const backupDirectory = await backupSddDirectory(this.root);
        const migrated = migrateFrom120(raw);
        await this.writeMigrationRecord(
          raw,
          "1.2.0",
          migrated,
          backupDirectory,
        );
        await this.write(migrated);
        return migrated;
      }
      if (raw.schemaVersion === "1.3.0") {
        const backupDirectory = await backupSddDirectory(this.root);
        const migrated = migrateFrom130(raw);
        await this.writeMigrationRecord(
          raw,
          "1.3.0",
          migrated,
          backupDirectory,
        );
        await this.write(migrated);
        return migrated;
      }
      if (raw.schemaVersion !== CURRENT_SCHEMA_VERSION) {
        throw new SddError(
          "E_STATE_CORRUPTED",
          `不支持的 state schemaVersion：${String(raw.schemaVersion ?? "unknown")}`,
          "sdd status",
        );
      }
      const parsed = workflowStateSchema.parse(raw);
      await this.validateChangeReference(parsed);
      return await this.normalizeActiveLoop(
        await this.normalizeTransientState(parsed),
      );
    } catch (error) {
      if (
        error instanceof SddError &&
        error.code === "E_STATE_CORRUPTED" &&
        error.message.includes("schemaVersion")
      ) {
        throw error;
      }
      try {
        // 优先回退到上一次完整写入时留下的备份。
        const backup = workflowStateSchema.parse(
          JSON.parse(await readFile(this.backupPath, "utf8")),
        );
        return { ...backup, recoveredFromBackup: true };
      } catch {
        // 主状态和备份都失效时，再依据制品完整性推断最近稳定阶段。
        const inferred = await this.recoverFromArtifacts();
        if (inferred !== null) return inferred;
        throw new SddError(
          "E_STATE_CORRUPTED",
          `无法读取状态文件或备份：${error instanceof Error ? error.message : String(error)}`,
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
    try {
      await handle.writeFile(`${JSON.stringify(validated, null, 2)}\n`, "utf8");
      await handle.sync();
    } finally {
      await handle.close();
    }
    // 用 rename 做最终替换，尽量避免直接覆盖导致半写入损坏。
    await rename(temporaryPath, this.path);
    try {
      const directory = await open(dirname(this.path), "r");
      try {
        await directory.sync();
      } finally {
        await directory.close();
      }
    } catch {
      // Directory fsync is not supported by every Windows filesystem.
    }
    await new AuditLogger(this.root).write({
      command: "state.write",
      phase: validated.currentPhase,
      result: "PASS",
      ...(validated.currentChangeId === null
        ? {}
        : { changeId: validated.currentChangeId }),
      message: `state version ${validated.version}`,
    });
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

  private async writeMigrationRecord(
    raw: Record<string, unknown>,
    from: string,
    migrated: WorkflowState,
    backupDirectory: string,
  ): Promise<void> {
    const backupPath = `${this.path}.migration.bak`;
    await writeFile(backupPath, `${JSON.stringify(raw, null, 2)}\n`, "utf8");
    await mkdir(join(this.root, ".sdd", "logs"), { recursive: true });
    await appendFile(
      join(this.root, ".sdd", "logs", "migration.log"),
      `${new Date().toISOString()} ${from} -> ${CURRENT_SCHEMA_VERSION}\n`,
      "utf8",
    );
    await new ArtifactWriter().write(
      join(this.root, ".sdd", "migration-report.md"),
      migrationReport({
        fromSchemaVersion: from,
        toSchemaVersion: CURRENT_SCHEMA_VERSION,
        nextVersion: migrated.version,
        backupPaths: [backupPath],
        backupDirectory,
        ...(typeof raw.version === "number"
          ? { previousVersion: raw.version }
          : {}),
      }),
      { fromSchemaVersion: from, toSchemaVersion: CURRENT_SCHEMA_VERSION },
    );
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
        `currentChangeId 指向的变更不存在：${state.currentChangeId}`,
        "sdd status",
      );
    }
  }

  private async normalizeTransientState(
    state: WorkflowState,
  ): Promise<WorkflowState> {
    const recovery = TRANSIENT_PHASE_RECOVERY[state.currentPhase];
    if (recovery === undefined) return state;
    if (await pathExists(join(this.root, ".sdd", "lock"))) return state;
    const normalized = workflowStateSchema.parse({
      ...state,
      currentPhase: "FAILED",
      previousPhase: recovery.previousPhase,
      inProgressPhase: state.currentPhase,
      failedCommand: recovery.command,
      failedReason:
        state.failedReason ??
        `检测到未完成的 ${recovery.command} 状态，已自动规范化为 FAILED`,
      interruptedCommand: null,
      recoverable: true,
      suggestedCommand: recovery.command,
      lastError: state.lastError ?? "E_INTERRUPTED",
      tasks:
        state.currentPhase === "BUILDING"
          ? Object.fromEntries(
              Object.entries(state.tasks).map(([taskId, status]) => [
                taskId,
                status === "BUILDING" ? "FAILED" : status,
              ]),
            )
          : state.tasks,
      version: state.version + 1,
      updatedAt: new Date().toISOString(),
    });
    await this.write(normalized);
    return normalized;
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
      // 从最靠后的稳定制品倒推阶段，避免跨阶段猜测未完成的状态。
      if (await pathExists(join(change, ".archived"))) phase = "ARCHIVED";
      else if (await reportPassed(join(change, "review-report.md")))
        phase = "REVIEW_READY";
      else if (await reportPassed(join(change, "verify-report.md")))
        phase = "VERIFY_READY";
      else if (await allTasksDone(change)) phase = "BUILD_READY";
      else if (await pathExists(join(change, "plan.json")))
        phase = "PLAN_READY";
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

  private async normalizeActiveLoop(
    state: WorkflowState,
  ): Promise<WorkflowState> {
    if (
      state.activeLoop === null ||
      typeof state.activeLoop !== "object" ||
      !("runId" in state.activeLoop) ||
      typeof state.activeLoop.runId !== "string"
    ) {
      return state;
    }
    const loop = state.activeLoop as {
      loopId: string;
      runId: string;
      status: string;
      recovered?: boolean;
    };
    if (await new LoopStore(this.root).hasRun(loop.runId)) return state;
    const recovered = workflowStateSchema.parse({
      ...state,
      activeLoop: {
        ...loop,
        status: "PAUSED",
        recovered: true,
      },
      version: state.version + 1,
      updatedAt: new Date().toISOString(),
    });
    await this.write(recovered);
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

async function allTasksDone(change: string): Promise<boolean> {
  try {
    const [plan, rawResults] = await Promise.all([
      readCompactPlan(change),
      readFile(join(change, "task-results.json"), "utf8"),
    ]);
    const tasks: unknown = plan.tasks;
    const results = JSON.parse(rawResults) as unknown;
    if (!Array.isArray(tasks) || tasks.length === 0 || !Array.isArray(results))
      return false;
    const taskIds = new Set<string>();
    for (const task of tasks) {
      const id =
        typeof task === "object" && task !== null
          ? (task as Record<string, unknown>).id
          : undefined;
      if (typeof id !== "string") return false;
      taskIds.add(id);
    }
    if (taskIds.size !== tasks.length || results.length !== taskIds.size)
      return false;
    const completed = new Set<string>();
    for (const result of results) {
      const taskId =
        typeof result === "object" && result !== null
          ? (result as Record<string, unknown>).taskId
          : undefined;
      if (
        typeof result !== "object" ||
        result === null ||
        typeof taskId !== "string" ||
        !["DONE", "SUCCEEDED"].includes(
          String((result as Record<string, unknown>).status),
        )
      )
        return false;
      completed.add(taskId);
    }
    return (
      completed.size === taskIds.size &&
      [...taskIds].every((id) => completed.has(id))
    );
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
      PLAN_READY: "sdd build next",
      BUILD_READY: "sdd verify",
      VERIFY_READY: "sdd review",
      REVIEW_READY: "sdd archive",
    }[phase as string] ?? null
  );
}

function migrationReport(input: {
  fromSchemaVersion: string;
  toSchemaVersion: string;
  previousVersion?: number;
  nextVersion: number;
  backupPaths: string[];
  backupDirectory: string;
}): string {
  return [
    "# 迁移报告",
    "",
    "## 概要",
    "",
    `- 源 schemaVersion：${input.fromSchemaVersion}`,
    `- 目标 schemaVersion：${input.toSchemaVersion}`,
    ...(input.previousVersion === undefined
      ? []
      : [`- 迁移前 version：${input.previousVersion}`]),
    `- 迁移后 version：${input.nextVersion}`,
    ...input.backupPaths.map((path) => `- 迁移备份文件：${path}`),
    `- .sdd 目录备份：${input.backupDirectory}`,
    `- 迁移时间：${new Date().toISOString()}`,
    "",
    "## 结果",
    "",
    "PASS",
    "",
  ].join("\n");
}

async function backupSddDirectory(root: string): Promise<string> {
  const sddPath = join(root, ".sdd");
  const backupName = ".sdd.migration.bak";
  const backupPath = join(root, backupName);
  await cp(sddPath, backupPath, {
    recursive: true,
    force: true,
    errorOnExist: false,
  });
  return backupName;
}

const TRANSIENT_PHASE_RECOVERY: Partial<
  Record<Phase, { command: string; previousPhase: Phase }>
> = {
  INITIALIZING: { command: "sdd init", previousPhase: "NOT_INITIALIZED" },
  INDEXING: { command: "sdd init", previousPhase: "NOT_INITIALIZED" },
  NEW_STARTED: { command: "sdd new", previousPhase: "INDEX_READY" },
  DESIGNING: { command: "sdd design", previousPhase: "SPEC_READY" },
  PLANNING: { command: "sdd plan", previousPhase: "DESIGN_READY" },
  BUILDING: { command: "sdd build", previousPhase: "PLAN_READY" },
  VERIFYING: { command: "sdd verify", previousPhase: "BUILD_READY" },
  REVIEWING: { command: "sdd review", previousPhase: "VERIFY_READY" },
  ARCHIVING: { command: "sdd archive", previousPhase: "REVIEW_READY" },
};

/** 将 1.2.0 schema 迁移到 1.3.0 */
function migrateFrom120(raw: Record<string, unknown>): WorkflowState {
  const phase = raw.currentPhase as string | undefined;
  const suggested = raw.suggestedCommand as string | undefined;
  const tasks = (raw.tasks as Record<string, string>) ?? {};

  if (
    phase === "BUILDING" &&
    (suggested?.includes("build next") || suggested?.includes("build complete"))
  ) {
    // 找到 BUILDING 状态的任务
    const buildingTasks = Object.entries(tasks).filter(
      ([, status]) => status === "BUILDING",
    );
    if (buildingTasks.length === 1) {
      const [taskId] = buildingTasks[0]!;
      return workflowStateSchema.parse({
        ...raw,
        schemaVersion: CURRENT_SCHEMA_VERSION,
        currentPhase: "PLAN_READY",
        inProgressPhase: null,
        tasks: { ...tasks, [taskId]: "PENDING" },
        activeLoop: null,
        pendingAgentTask: null,
        suggestedCommand: "sdd build next",
        version: typeof raw.version === "number" ? raw.version + 1 : 1,
        updatedAt: new Date().toISOString(),
      });
    }
    // 无法确定唯一 taskId → 标记 FAILED
    return workflowStateSchema.parse({
      ...raw,
      schemaVersion: CURRENT_SCHEMA_VERSION,
      currentPhase: "FAILED",
      failedReason:
        "旧 BUILDING 等待状态迁移失败：无法确定唯一 taskId，请人工检查",
      version: typeof raw.version === "number" ? raw.version + 1 : 1,
      updatedAt: new Date().toISOString(),
    });
  }

  // 其他 1.2.0 → 1.3.0：仅升级 schemaVersion
  return workflowStateSchema.parse({
    ...raw,
    schemaVersion: CURRENT_SCHEMA_VERSION,
    version: typeof raw.version === "number" ? raw.version + 1 : 1,
    updatedAt: new Date().toISOString(),
  });
}

/** 1.3.0 未保存可信 Git baseline，升级时不能伪造可提交的 handoff。 */
function migrateFrom130(raw: Record<string, unknown>): WorkflowState {
  const tasks = (raw.tasks as Record<string, string>) ?? {};
  const pending = raw.pendingAgentTask as
    | { taskId?: unknown }
    | null
    | undefined;
  const waiting = (raw.activeLoop as { waiting?: { taskId?: unknown } } | null)
    ?.waiting;
  const taskId =
    typeof pending?.taskId === "string"
      ? pending.taskId
      : typeof waiting?.taskId === "string"
        ? waiting.taskId
        : undefined;
  const hadHandoff =
    raw.currentPhase === "BUILD_WAITING_AGENT" || taskId !== undefined;
  const nextTasks =
    taskId === undefined
      ? Object.fromEntries(
          Object.entries(tasks).map(([id, status]) => [
            id,
            hadHandoff && status === "BUILDING" ? "PENDING" : status,
          ]),
        )
      : { ...tasks, [taskId]: "PENDING" };
  const activeLoop = raw.activeLoop as Record<string, unknown> | null;
  return workflowStateSchema.parse({
    ...raw,
    schemaVersion: CURRENT_SCHEMA_VERSION,
    version: typeof raw.version === "number" ? raw.version + 1 : 1,
    updatedAt: new Date().toISOString(),
    tasks: nextTasks,
    pendingAgentTask: null,
    currentPhase: hadHandoff ? "FAILED" : raw.currentPhase,
    failedCommand: hadHandoff ? "sdd build complete" : raw.failedCommand,
    failedReason: hadHandoff
      ? "旧 1.3.0 handoff 缺少可信 Git baseline，已拒绝恢复；请检查改动后重新执行 sdd build next"
      : raw.failedReason,
    lastError: hadHandoff ? "E_STATE_CORRUPTED" : raw.lastError,
    suggestedCommand: hadHandoff ? "sdd build next" : raw.suggestedCommand,
    activeLoop:
      activeLoop === null || !hadHandoff
        ? activeLoop
        : { ...activeLoop, status: "RUNNING", waiting: undefined },
  });
}
