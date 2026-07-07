export interface GitIsolationConfig {
  createBranch?: boolean;
  createWorktree?: boolean;
  branchPattern?: string;
  worktreeDir?: string;
}

export interface NormalizedGitIsolationConfig {
  createBranch: boolean;
  createWorktree: boolean;
  branchPattern: string;
  worktreeDir: string;
}

export interface ExecutionWorkspace {
  controlRoot: string;
  businessRoot: string;
  branchName: string | null;
  worktreePath: string | null;
  baselineCommit: string;
}
