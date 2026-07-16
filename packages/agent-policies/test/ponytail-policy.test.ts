import { describe, expect, it } from "vitest";

import {
  getPolicy,
  POLICIES,
  policyDigest,
  resolvePolicyBundle,
} from "../src/index.js";

const PONYTAIL_COMMIT = "14a0d79548d4de8fc2de95c1b94bb0de63a739d3";
const EXPECTED_DIGESTS = {
  "minimal-implementation":
    "sha256:fbbf892e0f153c37c7b1d607190b69e36681a476d9d09617857cd7539a92b355",
  "root-cause-minimal-fix":
    "sha256:07ea861d29bf1f7ca4aee367783118e03fba82ff63e1d27f2ddcbeb1f90c3c3f",
  "simplicity-review":
    "sha256:81c83665b0c34580fc92992c803b269143d851e206a7e49131000764d2b0c05c",
} as const;

describe("Ponytail-derived Policy", () => {
  it("注册三项改编 Policy 并记录固定来源", () => {
    const policies = [
      "minimal-implementation",
      "root-cause-minimal-fix",
      "simplicity-review",
    ] as const;
    expect(new Set(POLICIES.map(({ id }) => id)).size).toBe(POLICIES.length);
    for (const id of policies) {
      const policy = getPolicy(id);
      expect(policy.source).toMatchObject({
        project: "ponytail",
        upstreamCommit: PONYTAIL_COMMIT,
      });
      expect(policyDigest(policy)).toBe(EXPECTED_DIGESTS[id]);
    }
  });

  it("仅在需要的命令和失败场景渐进加载", () => {
    expect(
      resolvePolicyBundle({ command: "design" }).policies.map(({ id }) => id),
    ).toContain("minimal-implementation");
    expect(
      resolvePolicyBundle({ command: "review" }).policies.map(({ id }) => id),
    ).toContain("simplicity-review");
    expect(
      resolvePolicyBundle({
        command: "build",
        failureCode: "E_TEST_FAILED",
      }).policies.map(({ id }) => id),
    ).toContain("root-cause-minimal-fix");
    for (const command of ["verify", "archive"] as const) {
      const ids = resolvePolicyBundle({ command }).policies.map(({ id }) => id);
      expect(ids).not.toContain("minimal-implementation");
      expect(ids).not.toContain("simplicity-review");
    }
  });

  it("安全、TDD 与 Requirement 约束在最小实现之前", () => {
    const bundle = resolvePolicyBundle({ command: "build" });
    const ids = bundle.policies.map(({ id }) => id);
    expect(ids.indexOf("security-boundaries")).toBeLessThan(
      ids.indexOf("minimal-implementation"),
    );
    expect(ids.indexOf("tdd-task-execution")).toBeLessThan(
      ids.indexOf("minimal-implementation"),
    );
    expect(getPolicy("minimal-implementation").prompt).toContain(
      "不得因“更简单”而删除或弱化",
    );
    expect(getPolicy("minimal-implementation").prompt).toContain(
      "安全检查、数据完整性、兼容逻辑、迁移逻辑",
    );
  });
});
