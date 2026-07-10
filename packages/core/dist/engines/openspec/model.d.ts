export type DeltaOperation = "ADDED" | "MODIFIED" | "REMOVED";
export interface SpecScenario {
    id: string;
    title: string;
    given: string[];
    when: string[];
    then: string[];
}
export interface SpecRequirement {
    id: string;
    title: string;
    statement: string;
    operation: DeltaOperation;
    scenarios: SpecScenario[];
}
export interface SpecDocument {
    title: string;
    requirements: SpecRequirement[];
}
export interface SpecValidationFailure {
    code: "SPEC_NORMATIVE_KEYWORD_REQUIRED" | "SPEC_SCENARIO_REQUIRED" | "SPEC_DUPLICATE_ID" | "SPEC_DELTA_CONFLICT";
    path: string;
    message: string;
}
//# sourceMappingURL=model.d.ts.map