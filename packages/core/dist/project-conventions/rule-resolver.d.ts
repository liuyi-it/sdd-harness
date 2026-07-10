export type RuleHost = "codex" | "claude-code";
export interface ProjectRuleSnapshot {
    host: RuleHost;
    sources: Array<{
        path: string;
        scope: string;
        sha256: string;
        priority: number;
        content: string;
    }>;
    acknowledgement: "MUST_FOLLOW_PROJECT_RULES";
    hash: string;
}
export declare function resolveProjectRules(root: string, allowedFiles: string[], host?: RuleHost): Promise<ProjectRuleSnapshot>;
//# sourceMappingURL=rule-resolver.d.ts.map