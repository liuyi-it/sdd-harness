import { createHash } from "node:crypto";
import type { PhasePolicyDefinition } from "./types.js";

export function policyDigest(policy: PhasePolicyDefinition): string {
  return `sha256:${createHash("sha256")
    .update(
      JSON.stringify({
        id: policy.id,
        version: policy.version,
        prompt: normalize(policy.prompt),
        requiredEvidence: policy.requiredEvidence,
        completionCriteria: policy.completionCriteria,
      }),
    )
    .digest("hex")}`;
}

export function bundleDigest(bundle: {
  policies: unknown;
  instructions: string;
}): string {
  return `sha256:${createHash("sha256")
    .update(
      JSON.stringify({
        policies: bundle.policies,
        instructions: normalize(bundle.instructions),
      }),
    )
    .digest("hex")}`;
}

function normalize(value: string): string {
  return value.replace(/\r\n/g, "\n").trim();
}
