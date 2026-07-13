import type { PolicyBundle } from "@sdd-harness/agent-policies";
import type { PlanArtifacts, PlanningInput, TaskDefinition, TddPhase } from "../superpowers/protocol.js";
export interface DesignInput {
    spec: string;
    impact: string;
    codebaseSummary: string;
    packageStructure: string;
    architecture: string;
    policyBundle?: PolicyBundle;
    existingDesign?: string;
}
type MaybePromise<T> = T | Promise<T>;
export type { PlanArtifacts, TaskDefinition, TddPhase };
export declare class TddEngine {
    generateDesign(input: DesignInput): MaybePromise<string>;
    generatePlan(input: PlanningInput): MaybePromise<PlanArtifacts>;
}
//# sourceMappingURL=tdd-engine.d.ts.map