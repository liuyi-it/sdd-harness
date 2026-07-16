import { readFileSync } from "node:fs";
import { isAbsolute, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

import { PolicyError } from "./types.js";
import type { PhasePolicyDefinition, PhasePolicyId } from "./types.js";

const policiesRoot = resolve(
  fileURLToPath(new URL("../policies/", import.meta.url)),
);

const UPSTREAM_COMMITS = {
  superpowers: "d884ae04edebef577e82ff7c4e143debd0bbec99",
  "mattpocock-skills": "391a2701dd948f94f56a39f7533f8eea9a859c87",
  ponytail: "14a0d79548d4de8fc2de95c1b94bb0de63a739d3",
} as const;

export function loadPolicyPrompt(promptFile: string): string {
  if (
    promptFile.length === 0 ||
    isAbsolute(promptFile) ||
    promptFile.split(/[\\/]/u).includes("..")
  )
    throw new PolicyError(
      "E_POLICY_PATH_INVALID",
      `Policy 路径必须位于受控目录：${promptFile}`,
      { promptFile },
    );
  const path = resolve(policiesRoot, promptFile);
  if (!path.startsWith(`${policiesRoot}${sep}`))
    throw new PolicyError(
      "E_POLICY_PATH_INVALID",
      `Policy 路径越出受控目录：${promptFile}`,
      { promptFile },
    );
  try {
    return readFileSync(path, "utf8").replace(/\r\n/gu, "\n").trim();
  } catch {
    throw new PolicyError(
      "E_POLICY_CONTENT_MISSING",
      `Policy 内容不存在：${promptFile}`,
      { promptFile },
    );
  }
}

const definition = (
  id: PhasePolicyId,
  promptFile: string,
  commands: string[],
  evidence: string[] = [],
  source: PhasePolicyDefinition["source"]["project"] = "sdd-harness",
  completionCriteria: string[] = evidence.map((item) => `已提供并验证 ${item}`),
  adaptedFrom?: string[],
): PhasePolicyDefinition => ({
  id,
  version: "1.0.0",
  promptFile,
  appliesTo: { commands },
  prompt: loadPolicyPrompt(promptFile),
  requiredEvidence: evidence,
  completionCriteria,
  source: {
    project: source,
    ...(source === "sdd-harness"
      ? {}
      : { upstreamCommit: UPSTREAM_COMMITS[source] }),
    ...(adaptedFrom === undefined ? {} : { adaptedFrom }),
  },
});

export const POLICIES: readonly PhasePolicyDefinition[] = [
  definition("core-authority", "base/core-authority.md", [
    "new",
    "design",
    "plan",
    "build",
    "verify",
    "review",
    "archive",
  ]),
  definition("security-boundaries", "base/security-boundaries.md", [
    "new",
    "build",
  ]),
  definition(
    "evidence-before-completion",
    "base/evidence-before-completion.md",
    ["build", "verify", "review"],
  ),
  definition(
    "bounded-clarification",
    "new/bounded-clarification.md",
    ["new"],
    ["clarification-decision"],
    "mattpocock-skills",
  ),
  definition(
    "spec-authoring",
    "new/spec-authoring.md",
    ["new"],
    ["scenarios", "out-of-scope"],
    "mattpocock-skills",
  ),
  definition(
    "deep-module-design",
    "design/deep-module-design.md",
    ["design"],
    ["module-boundaries", "interfaces", "test-seams"],
    "mattpocock-skills",
  ),
  definition(
    "design-it-twice",
    "design/design-it-twice.md",
    [],
    ["alternatives"],
    "mattpocock-skills",
  ),
  definition(
    "tracer-bullet-planning",
    "plan/tracer-bullet-planning.md",
    ["plan"],
    ["user-visible-outcome", "acceptance-criteria"],
    "mattpocock-skills",
  ),
  definition(
    "expand-contract-migration",
    "plan/expand-contract-migration.md",
    ["plan"],
    ["migration-plan"],
    "mattpocock-skills",
  ),
  definition(
    "tdd-task-execution",
    "build/tdd-task-execution.md",
    ["build"],
    ["test-seam", "red-observation", "green-observation"],
    "superpowers",
  ),
  definition(
    "context-pack-consumer",
    "build/context-pack-consumer.md",
    ["build"],
    ["context-pack-read"],
    "mattpocock-skills",
  ),
  definition(
    "systematic-diagnosis",
    "failure/systematic-diagnosis.md",
    [],
    ["failure-signature", "reproduction"],
    "sdd-harness",
    undefined,
    ["superpowers/systematic-debugging", "mattpocock/diagnosing-bugs"],
  ),
  definition(
    "minimal-implementation",
    "shared/minimal-implementation.md",
    ["design", "plan", "build"],
    ["reuse-decision", "dependency-decision"],
    "ponytail",
    [
      "已确认是否存在可复用实现",
      "新增依赖具有明确依据",
      "没有引入无使用者的扩展结构",
    ],
    ["ponytail/AGENTS.md", "ponytail/skills/ponytail/SKILL.md"],
  ),
  definition(
    "root-cause-minimal-fix",
    "failure/root-cause-minimal-fix.md",
    [],
    ["root-cause", "affected-callers", "regression-test"],
    "ponytail",
    ["已定位共享根因", "已检查相邻调用路径", "已留下回归测试"],
    ["ponytail/AGENTS.md"],
  ),
  definition(
    "two-axis-review",
    "review/two-axis-review.md",
    ["review"],
    ["spec-axis", "standards-axis"],
    "mattpocock-skills",
  ),
  definition(
    "simplicity-review",
    "review/simplicity-review.md",
    ["review"],
    ["simplicity-findings"],
    "ponytail",
    ["已完成简洁性审查"],
    ["ponytail/skills/ponytail-review/SKILL.md"],
  ),
  definition(
    "handoff-and-traceability",
    "archive/handoff-and-traceability.md",
    ["archive"],
    ["policy-traceability"],
    "mattpocock-skills",
  ),
];

export const policiesByCommand = {
  new: [
    "core-authority",
    "security-boundaries",
    "bounded-clarification",
    "spec-authoring",
  ],
  design: ["core-authority", "deep-module-design", "minimal-implementation"],
  plan: [
    "core-authority",
    "tracer-bullet-planning",
    "expand-contract-migration",
    "minimal-implementation",
  ],
  build: [
    "core-authority",
    "security-boundaries",
    "context-pack-consumer",
    "tdd-task-execution",
    "minimal-implementation",
    "evidence-before-completion",
  ],
  verify: ["core-authority", "evidence-before-completion"],
  review: [
    "core-authority",
    "two-axis-review",
    "simplicity-review",
    "evidence-before-completion",
  ],
  archive: ["core-authority", "handoff-and-traceability"],
} as const satisfies Record<string, readonly PhasePolicyId[]>;

export const policiesByFailureCode = {
  E_TEST_FAILED: ["systematic-diagnosis", "root-cause-minimal-fix"],
  E_VERIFY_FAILED: ["systematic-diagnosis", "root-cause-minimal-fix"],
  E_REVIEW_FAILED: ["systematic-diagnosis", "root-cause-minimal-fix"],
  E_BUILD_RESULT_REJECTED: ["systematic-diagnosis", "root-cause-minimal-fix"],
} as const satisfies Record<string, readonly PhasePolicyId[]>;

export const policiesByActionType = {
  HIGH_RISK_DESIGN: ["design-it-twice"],
} as const satisfies Record<string, readonly PhasePolicyId[]>;

export function createPolicyRegistry(
  policies: readonly PhasePolicyDefinition[],
): ReadonlyMap<PhasePolicyId, PhasePolicyDefinition> {
  const registry = new Map<PhasePolicyId, PhasePolicyDefinition>();
  for (const policy of policies) {
    const existing = registry.get(policy.id);
    if (existing !== undefined) {
      throw new PolicyError(
        "E_POLICY_DUPLICATE",
        `Policy ${policy.id} 重复注册`,
        {
          id: policy.id,
          versions: `${existing.version},${policy.version}`,
        },
      );
    }
    registry.set(policy.id, policy);
  }
  return registry;
}

const policyMap = createPolicyRegistry(POLICIES);
export function getPolicy(id: PhasePolicyId): PhasePolicyDefinition {
  const policy = policyMap.get(id);
  if (policy === undefined)
    throw new PolicyError("E_POLICY_NOT_FOUND", `未知 Policy：${id}`, { id });
  return policy;
}
