import type {
  PhasePolicyId,
  PolicyBundle,
  PolicyRef,
} from "@sdd-harness/agent-protocol";

export type { PhasePolicyId, PolicyBundle, PolicyRef };

export interface PhasePolicyDefinition {
  id: PhasePolicyId;
  version: string;
  promptFile: string;
  appliesTo: {
    commands?: string[];
    phases?: string[];
    actionTypes?: string[];
    failureCodes?: string[];
  };
  prompt: string;
  requiredEvidence: string[];
  completionCriteria: string[];
  source: {
    project: "sdd-harness" | "superpowers" | "mattpocock-skills" | "ponytail";
    upstreamCommit?: string;
    adaptedFrom?: string[];
  };
}

export type PolicyErrorCode =
  | "E_POLICY_DUPLICATE"
  | "E_POLICY_NOT_FOUND"
  | "E_POLICY_MAPPING_NOT_FOUND"
  | "E_POLICY_PATH_INVALID"
  | "E_POLICY_CONTENT_MISSING";

export class PolicyError extends Error {
  constructor(
    readonly code: PolicyErrorCode,
    message: string,
    readonly details: Readonly<Record<string, string>> = {},
  ) {
    super(message);
    this.name = "PolicyError";
  }
}

export interface PolicyResolutionInput {
  command: string;
  phase?: string;
  actionType?: string;
  failureCode?: string;
  mode?: "AUTO" | "MANUAL";
}
