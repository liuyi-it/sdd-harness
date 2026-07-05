import { describe, expect, it } from "vitest";

import { TddEngine } from "../src/engines/tdd/tdd-engine.js";
import { detectProjectCommands } from "../src/engines/superpowers/project-commands.js";
import { SddError } from "../src/errors.js";

const threeRequirements = `# 订单规格

## ADDED Requirements

### Requirement: 创建订单
系统 SHALL 创建订单。

#### Scenario: 创建成功
- GIVEN 有效输入
- WHEN 创建订单
- THEN 返回订单

### Requirement: 取消订单
系统 SHALL 取消订单。

#### Scenario: 取消成功
- GIVEN 待处理订单
- WHEN 取消订单
- THEN 订单已取消

### Requirement: 查询订单
系统 SHALL 查询订单。

#### Scenario: 查询成功
- GIVEN 已有订单
- WHEN 查询订单
- THEN 返回订单详情`;

function generate(impact: string, codebaseSummary = `package.json\n${impact}`) {
  return new TddEngine().generatePlan({
    spec: threeRequirements,
    design: "订单模块保持现有边界。",
    impact,
    codebaseSummary,
  });
}

describe("Superpowers 原子计划器", () => {
  it("每个 requirement 生成严格依赖的四阶段任务链并关联场景", () => {
    const plan = generate(
      [
        "package.json",
        "src/create-order.ts",
        "test/create-order.test.ts",
        "src/cancel-order.ts",
        "test/cancel-order.test.ts",
        "src/query-order.ts",
        "test/query-order.test.ts",
      ].join("\n"),
    );

    expect(plan.tasks).toHaveLength(12);
    expect(plan.tasks.map((task) => task.phase)).toEqual([
      "RED",
      "GREEN",
      "REFACTOR",
      "VERIFY",
      "RED",
      "GREEN",
      "REFACTOR",
      "VERIFY",
      "RED",
      "GREEN",
      "REFACTOR",
      "VERIFY",
    ]);
    expect(plan.tasks.slice(0, 4).map((task) => task.id)).toEqual([
      "TASK-001-RED",
      "TASK-001-GREEN",
      "TASK-001-REFACTOR",
      "TASK-001-VERIFY",
    ]);
    expect(plan.tasks[1]!.dependsOn).toEqual(["TASK-001-RED"]);
    expect(plan.tasks[2]!.dependsOn).toEqual(["TASK-001-GREEN"]);
    expect(plan.tasks[3]!.dependsOn).toEqual(["TASK-001-REFACTOR"]);
    expect(plan.tasks[0]!.scenarios).toEqual(["REQ-001-SC-001"]);
    expect(plan.tasks[0]!.allowedFiles).toEqual([
      "src/create-order.ts",
      "test/create-order.test.ts",
    ]);
    expect(plan.tasks[1]!.allowedFiles).toEqual([
      "src/create-order.ts",
      "test/create-order.test.ts",
    ]);
  });

  it("文件范围不重叠时 requirement 链可以并行", () => {
    const plan = generate(
      "src/create-order.ts\ntest/create-order.test.ts\nsrc/cancel-order.ts\ntest/cancel-order.test.ts\nsrc/query-order.ts\ntest/query-order.test.ts",
    );

    expect(plan.tasks[4]!.dependsOn).toEqual([]);
    expect(plan.tasks[8]!.dependsOn).toEqual([]);
  });

  it("文件范围重叠时后一条链依赖前一条链 VERIFY", () => {
    const plan = generate(
      "src/order.ts\ntest/order.test.ts\nsrc/query-order.ts\ntest/query-order.test.ts",
    );

    expect(plan.tasks[4]!.dependsOn).toEqual(["TASK-001-VERIFY"]);
  });

  it("兼容旧 REQ 标题并提取场景", () => {
    const plan = new TddEngine().generatePlan({
      spec: "# Spec\n\n### REQ-007: Legacy\n\n#### Scenario: works\n- GIVEN x\n- WHEN y\n- THEN z",
      design: "legacy",
      impact: "src/legacy.ts\ntest/legacy.test.ts",
      codebaseSummary: "package.json\nsrc/legacy.ts\ntest/legacy.test.ts",
    });

    expect(plan.tasks[0]).toMatchObject({
      requirements: ["REQ-007"],
      scenarios: ["REQ-007-SC-001"],
    });
  });

  it("无法推导精确源码和测试范围时阻止计划推进", () => {
    expect(() =>
      new TddEngine().generatePlan({
        spec: threeRequirements,
        design: "unknown",
        impact: "Implementation and tests",
        codebaseSummary: "README.md",
      }),
    ).toThrowError(SddError);
    expect(() =>
      new TddEngine().generatePlan({
        spec: threeRequirements,
        design: "unknown",
        impact: "Implementation and tests",
        codebaseSummary: "README.md",
      }),
    ).toThrowError(/精确的源码与测试文件范围/);
  });

  it("tasks、Context Pack 和测试计划包含阶段及场景信息", () => {
    const plan = new TddEngine().generatePlan({
      spec: threeRequirements.split("### Requirement: 取消订单")[0]!,
      design: "src/create-order.ts",
      impact: "package.json\nsrc/create-order.ts\ntest/create-order.test.ts",
      codebaseSummary:
        "package.json\nsrc/create-order.ts\ntest/create-order.test.ts",
    });

    expect(plan.tasks).toHaveLength(4);
    for (const heading of [
      "Phase:",
      "Requirements:",
      "Scenarios:",
      "Depends On:",
      "Allowed Files:",
      "Expected New Files:",
      "Forbidden Files:",
      "Verification:",
      "Done Criteria:",
    ])
      expect(plan.tasksMarkdown).toContain(heading);
    expect(plan.contextPacks["TASK-001-RED"]).toContain("Phase: RED");
    expect(plan.contextPacks["TASK-001-RED"]).toContain("REQ-001-SC-001");
    expect(plan.contextPacks["TASK-001-RED"]).toContain(
      "先写测试并确认测试因缺少目标行为而失败",
    );
    expect(plan.testPlan).toContain("REQ-001-SC-001: 创建成功");
    expect(plan.testPlan).toContain("RED");
    expect(plan.testPlan).toContain("正向路径");
    expect(plan.testPlan).toContain("反向路径");
  });

  it("design 引用结构化 requirement、scenario、架构与真实候选文件", () => {
    const design = new TddEngine().generateDesign({
      spec: threeRequirements.split("### Requirement: 取消订单")[0]!,
      impact: "src/create-order.ts\ntest/create-order.test.ts",
      codebaseSummary: "package.json\nsrc/create-order.ts",
      packageStructure: "src\ntest",
      architecture: "CreateController -> CreateService",
    });

    expect(design).toContain("Requirement: 创建订单");
    expect(design).toContain("Scenario: 创建成功");
    expect(design).toContain("src/create-order.ts");
    expect(design).toContain("CreateController -> CreateService");
  });
});

describe("项目验证命令", () => {
  it("识别 Maven 和 npm 项目且稳定去重", () => {
    expect(detectProjectCommands(["pom.xml", "module/pom.xml"])).toEqual([
      "mvn test",
      "mvn verify",
    ]);
    expect(detectProjectCommands(["package.json", "package.json"])).toEqual([
      "npm test",
    ]);
    expect(detectProjectCommands(["pom.xml", "package.json"])).toEqual([
      "mvn test",
      "mvn verify",
      "npm test",
    ]);
  });
});
