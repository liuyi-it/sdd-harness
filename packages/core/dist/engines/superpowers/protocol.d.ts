export type TddPhase = "RED" | "GREEN" | "REFACTOR" | "VERIFY";
export interface TaskDefinition {
    id: string;
    title: string;
    phase: TddPhase;
    status: "PENDING" | "BUILDING" | "DONE" | "FAILED" | "SKIPPED";
    requirements: string[];
    scenarios: string[];
    dependsOn: string[];
    allowedFiles: string[];
    expectedNewFiles: string[];
    forbiddenFiles: string[];
    verification: string[];
    doneCriteria: string[];
    /** 第五期任务增强字段，随任务统一持久化到 plan.json。 */
    sliceType?: "VERTICAL" | "EXPAND" | "MIGRATE" | "CONTRACT" | "REPAIR";
    userVisibleOutcome?: string;
    acceptanceCriteria?: string[];
    testSeam?: string;
    policyRefs?: PolicyRef[];
    failureContext?: {
        source: "VERIFY" | "REVIEW" | "BUILD";
        errorCode: string;
        failingCommand?: string;
        findingIds?: string[];
        previousRunId: string;
    };
}
export interface PlanArtifacts {
    tasks: TaskDefinition[];
    tasksMarkdown: string;
    testPlan: string;
    context: string;
    contextPacks: Record<string, string>;
}
export interface PlanningInput {
    spec: string;
    design: string;
    impact: string;
    codebaseSummary: string;
    policyBundle?: PolicyBundle;
    existingPlan?: {
        tasksMarkdown: string;
        testPlan: string;
        context: string;
    };
}
import type { PolicyBundle, PolicyRef } from "@sdd-harness/agent-policies";
//# sourceMappingURL=protocol.d.ts.map