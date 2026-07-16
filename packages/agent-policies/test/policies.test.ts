import { describe, expect, it } from "vitest";
import {
  compileBaseSkill,
  createPolicyRegistry,
  PolicyError,
  loadPolicyPrompt,
  policyDigest,
  POLICIES,
  resolvePolicyBundle,
} from "../src/index.js";

describe("agent policies", () => {
  it("Policy ID 唯一且摘要稳定", () => {
    expect(new Set(POLICIES.map((policy) => policy.id)).size).toBe(
      POLICIES.length,
    );
    expect(policyDigest(POLICIES[0]!)).toBe(policyDigest(POLICIES[0]!));
    expect(POLICIES.every((policy) => policy.promptFile.endsWith(".md"))).toBe(
      true,
    );
    expect(policyDigest(POLICIES[0]!)).toBe(
      "sha256:f604582d26ea54ab0e7946aac7e5599a0e7c332481c89dcf5e548f42bf418e07",
    );
    expect(
      policyDigest({ ...POLICIES[0]!, prompt: `${POLICIES[0]!.prompt}\n篡改` }),
    ).not.toBe(policyDigest(POLICIES[0]!));
  });

  it("所有上游改编 Policy 都记录固定 commit", () => {
    for (const policy of POLICIES.filter(
      ({ source }) => source.project !== "sdd-harness",
    ))
      expect(policy.source.upstreamCommit).toMatch(/^[a-f0-9]{40}$/u);
  });

  it.each(["../secret.md", "/tmp/secret.md", "base/../../secret.md"])(
    "拒绝越出受控目录的 Policy 路径 %s",
    (path) => {
      expect(() => loadPolicyPrompt(path)).toThrowError(
        expect.objectContaining({ code: "E_POLICY_PATH_INVALID" }),
      );
    },
  );

  it("缺失 Policy 内容返回结构化错误", () => {
    expect(() => loadPolicyPrompt("base/missing.md")).toThrowError(
      expect.objectContaining({ code: "E_POLICY_CONTENT_MISSING" }),
    );
  });

  it("各命令按固定顺序合并 Policy", () => {
    const bundle = resolvePolicyBundle({
      command: "build",
      phase: "PLAN_READY",
    });
    expect(bundle.policies.map((policy) => policy.id)).toEqual([
      "core-authority",
      "security-boundaries",
      "context-pack-consumer",
      "tdd-task-execution",
      "minimal-implementation",
      "evidence-before-completion",
    ]);
    expect(bundle.instructions).toContain("不得修改 `.sdd/state.json`");
  });

  it("仅在失败时追加系统化诊断 Policy", () => {
    expect(
      resolvePolicyBundle({ command: "build" }).policies.map(({ id }) => id),
    ).not.toContain("systematic-diagnosis");
    expect(
      resolvePolicyBundle({
        command: "build",
        failureCode: "E_TEST_FAILED",
      }).policies.map(({ id }) => id),
    ).toContain("systematic-diagnosis");
  });

  it("仅对高风险设计追加双方案 Policy", () => {
    expect(
      resolvePolicyBundle({ command: "design" }).policies.map(({ id }) => id),
    ).not.toContain("design-it-twice");
    expect(
      resolvePolicyBundle({
        command: "design",
        actionType: "HIGH_RISK_DESIGN",
      }).policies.map(({ id }) => id),
    ).toContain("design-it-twice");
  });

  it("拒绝重复注册并对缺失映射返回结构化错误", () => {
    expect(() => createPolicyRegistry([POLICIES[0]!, POLICIES[0]!])).toThrow(
      PolicyError,
    );
    try {
      resolvePolicyBundle({ command: "unknown" });
      throw new Error("预期 resolvePolicyBundle 失败");
    } catch (error) {
      expect(error).toBeInstanceOf(PolicyError);
      expect((error as PolicyError).code).toBe("E_POLICY_MAPPING_NOT_FOUND");
    }
  });

  it("常驻 Skill 保持最小化，不内联阶段 TDD 细则", () => {
    expect(compileBaseSkill()).not.toContain("RED 观察");
  });
});
