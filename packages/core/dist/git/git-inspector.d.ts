export interface GitSnapshot {
    available: boolean;
    files: string[];
    hashes: Record<string, string>;
}
export declare class GitInspector {
    private readonly root;
    constructor(root: string);
    snapshot(): Promise<GitSnapshot>;
    delta(before: GitSnapshot, after: GitSnapshot): string[];
}
export declare function snapshotFromJson(input: unknown): GitSnapshot | null;
//# sourceMappingURL=git-inspector.d.ts.map