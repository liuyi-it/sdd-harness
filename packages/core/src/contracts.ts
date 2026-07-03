export const COMMANDS = [
  "init",
  "auto",
  "new",
  "design",
  "plan",
  "build",
  "verify",
  "review",
  "archive",
  "status",
] as const;

export type CommandName = (typeof COMMANDS)[number];

export const PHASES = [
  "NOT_INITIALIZED",
  "INITIALIZING",
  "INDEXING",
  "INDEX_READY",
  "NEW_STARTED",
  "CLARIFYING",
  "SPEC_READY",
  "DESIGNING",
  "DESIGN_READY",
  "PLANNING",
  "PLAN_READY",
  "BUILDING",
  "BUILD_READY",
  "VERIFYING",
  "VERIFY_READY",
  "REVIEWING",
  "REVIEW_READY",
  "ARCHIVING",
  "ARCHIVED",
  "FAILED",
  "PAUSED",
] as const;

export type Phase = (typeof PHASES)[number];

export const ERROR_EXIT_CODES = {
  E_NOT_INITIALIZED: 3,
  E_INVALID_PHASE_COMMAND: 3,
  E_ACTIVE_CHANGE_EXISTS: 3,
  E_MISSING_CHANGE: 4,
  E_MISSING_ARTIFACT: 4,
  E_INDEX_NOT_READY: 5,
  E_COMPONENT_UNAVAILABLE: 5,
  E_COMPONENT_INTEGRITY_FAILED: 10,
  E_DEGRADED_MODE: 0,
  E_UNRESOLVED_BLOCKER: 6,
  E_VERIFY_REQUIRED: 3,
  E_REVIEW_REQUIRED: 3,
  E_VERIFY_FAILED: 7,
  E_REVIEW_FAILED: 8,
  E_ARCHIVED_READONLY: 3,
  E_CONCURRENT_RUN: 9,
  E_LOCK_TIMEOUT: 9,
  E_TIMEOUT: 124,
  E_INTERRUPTED: 130,
  E_STATE_CORRUPTED: 1,
  E_SECURITY_BLOCKED: 10,
  E_PATH_OUTSIDE_REPO: 10,
  E_SYMLINK_BLOCKED: 10,
  E_PARALLEL_FILE_CONFLICT: 3,
} as const;

export type ErrorCode = keyof typeof ERROR_EXIT_CODES;

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
  warnings?: string[];
  error?: CommandError;
}

export interface SddCore {
  execute(request: CommandRequest): Promise<CommandResult>;
}
