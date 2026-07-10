import type { ProjectRuleSnapshot } from "../project-conventions/rule-resolver.js";
export interface ContextPackMetadata {
    codebaseIndexHash: string;
    sourceArtifactHash: string;
    projectRulesHash: string;
    projectConventionsHash: string;
}
export declare function renderContextPack(input: {
    body: string;
    rules: ProjectRuleSnapshot;
    codebaseSummary: string;
    spec: string;
    design: string;
    impact: string;
    tasksMarkdown: string;
    tasksJson: string;
    projectConventionsHash: string;
}): string;
export declare function readContextPackMetadata(content: string): ContextPackMetadata;
export declare function stripManagedSections(content: string): string;
//# sourceMappingURL=context-pack.d.ts.map