import { z } from "zod";
import { type Phase } from "../contracts.js";
export declare const workflowStateSchema: z.ZodObject<{
    schemaVersion: z.ZodLiteral<"1.4.0">;
    version: z.ZodNumber;
    updatedAt: z.ZodString;
    initialized: z.ZodBoolean;
    currentChangeId: z.ZodNullable<z.ZodString>;
    currentRunId: z.ZodNullable<z.ZodString>;
    activeLoop: z.ZodNullable<z.ZodObject<{
        loopId: z.ZodString;
        runId: z.ZodString;
        status: z.ZodEnum<{
            RUNNING: "RUNNING";
            WAITING_AGENT: "WAITING_AGENT";
            PAUSED: "PAUSED";
            FAILED: "FAILED";
            SUCCEEDED: "SUCCEEDED";
            ABORTED: "ABORTED";
            ARCHIVED: "ARCHIVED";
        }>;
        waiting: z.ZodOptional<z.ZodObject<{
            reason: z.ZodEnum<{
                AGENT_TASK_EXECUTION: "AGENT_TASK_EXECUTION";
                CLARIFICATION: "CLARIFICATION";
                HUMAN_REVIEW: "HUMAN_REVIEW";
            }>;
            taskId: z.ZodOptional<z.ZodString>;
            resultFile: z.ZodOptional<z.ZodString>;
            since: z.ZodString;
        }, z.core.$strip>>;
        recovered: z.ZodOptional<z.ZodBoolean>;
        lastDecision: z.ZodOptional<z.ZodString>;
    }, z.core.$strip>>;
    pendingAgentTask: z.ZodDefault<z.ZodNullable<z.ZodObject<{
        taskId: z.ZodString;
        resultFile: z.ZodString;
        since: z.ZodString;
        gitBaseline: z.ZodObject<{
            available: z.ZodLiteral<true>;
            files: z.ZodArray<z.ZodString>;
            hashes: z.ZodRecord<z.ZodString, z.ZodString>;
            tracked: z.ZodArray<z.ZodString>;
        }, z.core.$strip>;
    }, z.core.$strip>>>;
    currentPhase: z.ZodEnum<{
        PAUSED: "PAUSED";
        FAILED: "FAILED";
        ARCHIVED: "ARCHIVED";
        NOT_INITIALIZED: "NOT_INITIALIZED";
        INITIALIZING: "INITIALIZING";
        INDEXING: "INDEXING";
        INDEX_READY: "INDEX_READY";
        NEW_STARTED: "NEW_STARTED";
        CLARIFYING: "CLARIFYING";
        SPEC_READY: "SPEC_READY";
        DESIGNING: "DESIGNING";
        DESIGN_READY: "DESIGN_READY";
        PLANNING: "PLANNING";
        PLAN_READY: "PLAN_READY";
        BUILDING: "BUILDING";
        BUILD_WAITING_AGENT: "BUILD_WAITING_AGENT";
        BUILD_READY: "BUILD_READY";
        VERIFYING: "VERIFYING";
        VERIFY_READY: "VERIFY_READY";
        REVIEWING: "REVIEWING";
        REVIEW_READY: "REVIEW_READY";
        ARCHIVING: "ARCHIVING";
    }>;
    indexStatus: z.ZodEnum<{
        INDEXING: "INDEXING";
        INDEX_READY: "INDEX_READY";
        MISSING: "MISSING";
        STALE: "STALE";
        UNAVAILABLE: "UNAVAILABLE";
    }>;
    codebaseProvider: z.ZodString;
    degraded: z.ZodBoolean;
    degradedReason: z.ZodNullable<z.ZodString>;
    lastCommand: z.ZodNullable<z.ZodString>;
    lastError: z.ZodNullable<z.ZodString>;
    previousPhase: z.ZodNullable<z.ZodEnum<{
        PAUSED: "PAUSED";
        FAILED: "FAILED";
        ARCHIVED: "ARCHIVED";
        NOT_INITIALIZED: "NOT_INITIALIZED";
        INITIALIZING: "INITIALIZING";
        INDEXING: "INDEXING";
        INDEX_READY: "INDEX_READY";
        NEW_STARTED: "NEW_STARTED";
        CLARIFYING: "CLARIFYING";
        SPEC_READY: "SPEC_READY";
        DESIGNING: "DESIGNING";
        DESIGN_READY: "DESIGN_READY";
        PLANNING: "PLANNING";
        PLAN_READY: "PLAN_READY";
        BUILDING: "BUILDING";
        BUILD_WAITING_AGENT: "BUILD_WAITING_AGENT";
        BUILD_READY: "BUILD_READY";
        VERIFYING: "VERIFYING";
        VERIFY_READY: "VERIFY_READY";
        REVIEWING: "REVIEWING";
        REVIEW_READY: "REVIEW_READY";
        ARCHIVING: "ARCHIVING";
    }>>;
    inProgressPhase: z.ZodNullable<z.ZodEnum<{
        PAUSED: "PAUSED";
        FAILED: "FAILED";
        ARCHIVED: "ARCHIVED";
        NOT_INITIALIZED: "NOT_INITIALIZED";
        INITIALIZING: "INITIALIZING";
        INDEXING: "INDEXING";
        INDEX_READY: "INDEX_READY";
        NEW_STARTED: "NEW_STARTED";
        CLARIFYING: "CLARIFYING";
        SPEC_READY: "SPEC_READY";
        DESIGNING: "DESIGNING";
        DESIGN_READY: "DESIGN_READY";
        PLANNING: "PLANNING";
        PLAN_READY: "PLAN_READY";
        BUILDING: "BUILDING";
        BUILD_WAITING_AGENT: "BUILD_WAITING_AGENT";
        BUILD_READY: "BUILD_READY";
        VERIFYING: "VERIFYING";
        VERIFY_READY: "VERIFY_READY";
        REVIEWING: "REVIEWING";
        REVIEW_READY: "REVIEW_READY";
        ARCHIVING: "ARCHIVING";
    }>>;
    failedCommand: z.ZodNullable<z.ZodString>;
    failedReason: z.ZodOptional<z.ZodNullable<z.ZodString>>;
    interruptedCommand: z.ZodNullable<z.ZodString>;
    recoverable: z.ZodOptional<z.ZodBoolean>;
    suggestedCommand: z.ZodNullable<z.ZodString>;
    workspace: z.ZodOptional<z.ZodNullable<z.ZodObject<{
        branchName: z.ZodNullable<z.ZodString>;
        worktreePath: z.ZodNullable<z.ZodString>;
        baselineCommit: z.ZodString;
    }, z.core.$strip>>>;
    tasks: z.ZodRecord<z.ZodString, z.ZodEnum<{
        FAILED: "FAILED";
        BUILDING: "BUILDING";
        PENDING: "PENDING";
        DONE: "DONE";
        SKIPPED: "SKIPPED";
    }>>;
    artifacts: z.ZodRecord<z.ZodString, z.ZodEnum<{
        MISSING: "MISSING";
        STALE: "STALE";
        READY: "READY";
        CANDIDATE: "CANDIDATE";
    }>>;
    recoveredFromBackup: z.ZodOptional<z.ZodBoolean>;
}, z.core.$strip>;
export type WorkflowState = z.infer<typeof workflowStateSchema>;
export declare function createInitialState(): WorkflowState;
export declare class StateStore {
    readonly root: string;
    readonly path: string;
    readonly backupPath: string;
    constructor(root: string);
    read(): Promise<WorkflowState>;
    write(state: WorkflowState): Promise<void>;
    update(updater: (state: WorkflowState) => WorkflowState): Promise<WorkflowState>;
    private writeMigrationRecord;
    private validateChangeReference;
    private normalizeTransientState;
    private recoverFromArtifacts;
    private normalizeActiveLoop;
}
export declare function isPhase(value: string): value is Phase;
//# sourceMappingURL=state-store.d.ts.map