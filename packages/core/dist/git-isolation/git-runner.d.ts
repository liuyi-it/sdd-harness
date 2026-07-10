type ExecResult = Promise<{
    stdout: string;
    stderr: string;
}>;
type ExecFunction = (file: string, args: string[], options: {
    cwd: string;
    encoding: "utf8";
}) => ExecResult;
export interface GitWorktreeEntry {
    path: string;
    head: string;
    branch: string | null;
}
export declare class GitRunner {
    private readonly exec;
    constructor(options?: {
        exec?: ExecFunction;
    });
    run(cwd: string, args: string[]): Promise<string>;
    branchCurrent(cwd: string): Promise<string>;
    revParse(cwd: string, target: "HEAD" | "--show-toplevel"): Promise<string>;
    statusPorcelain(cwd: string): Promise<string>;
    worktreeAdd(cwd: string, path: string, branch: string, commit: string): Promise<void>;
    worktreeList(cwd: string): Promise<GitWorktreeEntry[]>;
    private assertAllowed;
}
export {};
//# sourceMappingURL=git-runner.d.ts.map