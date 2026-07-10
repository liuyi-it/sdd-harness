import { GitRunner } from "./git-runner.js";
import type { ExecutionWorkspace, GitIsolationConfig, NormalizedGitIsolationConfig } from "./model.js";
export declare function normalizeGitIsolationConfig(config?: GitIsolationConfig): NormalizedGitIsolationConfig;
export declare class GitIsolationManager {
    private readonly controlRoot;
    private readonly git;
    private readonly config;
    constructor(controlRoot: string, config?: GitIsolationConfig, git?: GitRunner);
    plan(changeId: string): Promise<ExecutionWorkspace>;
    ensure(changeId: string): Promise<ExecutionWorkspace>;
    private assertChangeId;
}
//# sourceMappingURL=manager.d.ts.map