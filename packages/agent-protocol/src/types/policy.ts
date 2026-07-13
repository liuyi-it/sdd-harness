export type PhasePolicyId =
  | "core-authority"
  | "security-boundaries"
  | "evidence-before-completion"
  | "bounded-clarification"
  | "spec-authoring"
  | "deep-module-design"
  | "design-it-twice"
  | "tracer-bullet-planning"
  | "expand-contract-migration"
  | "tdd-task-execution"
  | "context-pack-consumer"
  | "systematic-diagnosis"
  | "two-axis-review"
  | "handoff-and-traceability";

export interface PolicyRef {
  id: PhasePolicyId;
  version: string;
  digest: string;
}

export interface PolicyBundle {
  schemaVersion: "1.0.0";
  policies: PolicyRef[];
  instructions: string;
  requiredEvidence: string[];
  completionCriteria: string[];
}
