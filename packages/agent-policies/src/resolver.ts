import { policyDigest } from "./digest.js";
import {
  getPolicy,
  policiesByActionType,
  policiesByCommand,
  policiesByFailureCode,
} from "./registry.js";
import { PolicyError } from "./types.js";
import type {
  PhasePolicyId,
  PolicyBundle,
  PolicyResolutionInput,
} from "./types.js";

export function resolvePolicyBundle(
  input: PolicyResolutionInput,
): PolicyBundle {
  const commandPolicyIds = lookupPolicyIds(policiesByCommand, input.command);
  if (commandPolicyIds === undefined) {
    throw new PolicyError(
      "E_POLICY_MAPPING_NOT_FOUND",
      `命令 ${input.command} 没有可用 Policy`,
      { command: input.command },
    );
  }
  const failurePolicyIds = lookupPolicyIds(
    policiesByFailureCode,
    input.failureCode,
  );
  const actionPolicyIds = lookupPolicyIds(
    policiesByActionType,
    input.actionType,
  );
  const ids = [
    ...commandPolicyIds,
    ...(failurePolicyIds ?? []),
    ...(actionPolicyIds ?? []),
  ].filter((id, index, values) => values.indexOf(id) === index);
  const selected = ids.map((id) => getPolicy(id));

  return {
    schemaVersion: "1.0.0",
    policies: selected.map((policy) => ({
      id: policy.id,
      version: policy.version,
      digest: policyDigest(policy),
    })),
    instructions: selected
      .map((policy) => `## ${policy.id}\n\n${policy.prompt}`)
      .join("\n\n"),
    requiredEvidence: [
      ...new Set(selected.flatMap((policy) => policy.requiredEvidence)),
    ],
    completionCriteria: [
      ...new Set(selected.flatMap((policy) => policy.completionCriteria)),
    ],
  };
}

function lookupPolicyIds(
  mapping: Readonly<Record<string, readonly PhasePolicyId[]>>,
  key: string | undefined,
): readonly PhasePolicyId[] | undefined {
  return key === undefined ? undefined : mapping[key];
}
