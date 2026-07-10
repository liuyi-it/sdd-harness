export declare const COMMANDS: readonly ["init", "auto", "new", "design", "plan", "build", "verify", "review", "archive", "status", "codebase"];
/**
 * 这里定义所有对外稳定契约：
 * - 命令集合
 * - 状态枚举
 * - 错误码到退出码的映射
 * - Core 请求/响应结构
 */
export type CommandName = (typeof COMMANDS)[number];
export declare const PHASES: readonly ["NOT_INITIALIZED", "INITIALIZING", "INDEXING", "INDEX_READY", "NEW_STARTED", "CLARIFYING", "SPEC_READY", "DESIGNING", "DESIGN_READY", "PLANNING", "PLAN_READY", "BUILDING", "BUILD_WAITING_AGENT", "BUILD_READY", "VERIFYING", "VERIFY_READY", "REVIEWING", "REVIEW_READY", "ARCHIVING", "ARCHIVED", "FAILED", "PAUSED"];
export type Phase = (typeof PHASES)[number];
export declare const ERROR_EXIT_CODES: {
    readonly E_NOT_INITIALIZED: 3;
    readonly E_INVALID_PHASE_COMMAND: 3;
    readonly E_ACTIVE_CHANGE_EXISTS: 3;
    readonly E_MISSING_CHANGE: 4;
    readonly E_MISSING_ARTIFACT: 4;
    readonly E_INDEX_NOT_READY: 5;
    readonly E_COMPONENT_UNAVAILABLE: 5;
    readonly E_COMPONENT_INTEGRITY_FAILED: 10;
    readonly E_DEGRADED_MODE: 0;
    readonly E_UNRESOLVED_BLOCKER: 6;
    readonly E_VERIFY_REQUIRED: 3;
    readonly E_REVIEW_REQUIRED: 3;
    readonly E_VERIFY_FAILED: 7;
    readonly E_TDD_EVIDENCE_REQUIRED: 7;
    readonly E_REVIEW_FAILED: 8;
    readonly E_ARCHIVED_READONLY: 3;
    readonly E_CONCURRENT_RUN: 9;
    readonly E_LOCK_TIMEOUT: 9;
    readonly E_TIMEOUT: 124;
    readonly E_INTERRUPTED: 130;
    readonly E_STATE_CORRUPTED: 1;
    readonly E_SECURITY_BLOCKED: 10;
    readonly E_PATH_OUTSIDE_REPO: 10;
    readonly E_SYMLINK_BLOCKED: 10;
    readonly E_PARALLEL_FILE_CONFLICT: 3;
};
export type ErrorCode = keyof typeof ERROR_EXIT_CODES;
/** 三期统一退出码，CLI 进程退出码必须等于 CommandResult.exitCode */
export declare const ExitCode: {
    readonly SUCCESS: 0;
    readonly GENERAL_ERROR: 1;
    readonly INVALID_ARGS: 2;
    readonly STATE_CONFLICT: 3;
    readonly SCHEMA_VALIDATION_FAILED: 4;
    readonly SECURITY_BLOCKED: 5;
    readonly COMPONENT_UNAVAILABLE: 6;
    readonly TIMEOUT: 124;
};
export type ExitCodeValue = (typeof ExitCode)[keyof typeof ExitCode];
/** CLI 结构化警告 */
export interface CliWarning {
    /** 警告码，如 "W_CODEBASE_MEMORY_UNAVAILABLE" */
    code: string;
    /** 人类可读警告信息 */
    message: string;
    /** 建议的下一步命令，如 "sdd codebase doctor" */
    next?: string;
    /** 额外诊断详情 */
    details?: Record<string, unknown>;
}
/** Agent 行动要求 — 三期 build next 返回此结构 */
export interface AgentActionRequired {
    type: "AGENT_TASK_EXECUTION";
    taskId: string;
    changeId: string;
    contextPack: string;
    allowedFiles: string[];
    expectedNewFiles: string[];
    forbiddenFiles: string[];
    verification: Array<{
        command: string;
        args: string[];
    }>;
    resultFile: string;
    codebase: {
        provider: "codebase-memory-mcp" | "fallback-file-scan";
        degraded: boolean;
    };
}
export interface CommandRequest {
    command: CommandName;
    cwd: string;
    args?: Record<string, unknown>;
    signal?: AbortSignal;
}
export interface CommandError {
    code: ErrorCode;
    message: string;
    next?: string;
}
export interface CommandResult {
    ok: boolean;
    state: Phase;
    exitCode: number;
    changeId?: string;
    next?: string;
    data?: unknown;
    rendered?: {
        format: "json" | "text";
        content: string;
    };
    warnings?: Array<string | CliWarning>;
    /** Agent 行动要求 — build next 时返回，指导 Agent 执行任务 */
    actionRequired?: AgentActionRequired;
    error?: CommandError;
}
export interface SddCore {
    execute(request: CommandRequest): Promise<CommandResult>;
}
//# sourceMappingURL=contracts.d.ts.map