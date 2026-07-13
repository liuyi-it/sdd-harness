import type { SpecDocument } from "../openspec/model.js";
export interface ClarifyingQuestion {
    id: string;
    severity: "BLOCKER" | "IMPORTANT" | "OPTIONAL";
    question: string;
}
export interface SpecAnalysis {
    questions: ClarifyingQuestion[];
}
export interface GenerateSpecInput {
    requirement: string;
    codebaseSummary: string;
    answers?: Record<string, string>;
    policyBundle?: PolicyBundle;
    existingSpec?: {
        spec: string;
        delta: string;
        model: SpecDocument;
    };
}
export interface SpecArtifacts {
    proposal: string;
    impact: string;
    questions: string;
    answers: string;
    assumptions: string;
    spec: string;
    delta: string;
    model: SpecDocument;
}
type MaybePromise<T> = T | Promise<T>;
export declare class SpecEngine {
    analyze(requirement: string, answers?: Record<string, string>): SpecAnalysis;
    generate(input: GenerateSpecInput): MaybePromise<SpecArtifacts>;
}
import type { PolicyBundle } from "@sdd-harness/agent-policies";
export {};
//# sourceMappingURL=spec-engine.d.ts.map