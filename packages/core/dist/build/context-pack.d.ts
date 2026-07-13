import type { ProjectRuleSnapshot } from "../project-conventions/rule-resolver.js";
import type { PolicyBundle } from "@sdd-harness/agent-policies";
export interface ContextPackMetadata {
    schemaVersion: "2.0.0";
    codebaseIndexHash: string;
    sourceArtifactHash: string;
    projectRulesHash: string;
    projectConventionsHash: string;
    policyBundleHash?: string;
    contextPackDigest: string;
}
export interface ContextPackReferences {
    spec: string;
    design: string;
    plan: string;
    impact: string;
    codebase: string;
    domain?: string;
    adr?: string[];
    previousRun?: string;
}
export interface ContextPackTask {
    taskId: string;
    objective: string;
    userVisibleOutcome: string;
    requiredFiles: string[];
    allowedFiles: string[];
    forbiddenFiles: string[];
    verification: string[];
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
    references: ContextPackReferences;
    task: ContextPackTask;
    policyBundle?: PolicyBundle;
}): string;
export declare function readContextPackMetadata(content: string): ContextPackMetadata;
export declare function stripManagedSections(content: string): string;
export declare function verifyContextPackDigest(content: string): boolean;
//# sourceMappingURL=context-pack.d.ts.map