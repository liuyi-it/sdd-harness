import { describe, expect, it } from "vitest";

import { TddEngine } from "../src/engines/tdd/tdd-engine.js";
import { extractPaths } from "../src/engines/superpowers/planner.js";
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

const explicitlyMappedImpact = [
  "package.json",
  "REQ-001 创建订单: src/create-order.ts test/create-order.test.ts",
  "REQ-002 取消订单: src/cancel-order.ts test/cancel-order.test.ts",
  "REQ-003 查询订单: src/query-order.ts test/query-order.test.ts",
].join("\n");

describe("Superpowers 原子计划器", () => {
  it.each([
    ["Unix 绝对路径", "`/tmp/src/order.ts`"],
    ["Windows drive 路径", '"C:\\src\\order.ts"'],
    ["Windows UNC 路径", "(\\\\server\\share\\order.ts)"],
  ])("Markdown 包裹的%s不会被截断为相对路径", (_name, text) => {
    expect(extractPaths(text)).toEqual([]);
  });

  it("提取 Markdown 包裹的安全相对路径", () => {
    expect(
      extractPaths("- `src/orders/order.ts` 和 (test/order.test.ts)"),
    ).toEqual(["src/orders/order.ts", "test/order.test.ts"]);
  });

  it("每个 requirement 生成严格依赖的四阶段任务链并关联场景", () => {
    const plan = generate(explicitlyMappedImpact);

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
    expect(plan.tasks.every((task) => task.sliceType === "VERTICAL")).toBe(
      true,
    );
    expect(
      plan.tasks.every((task) =>
        task.userVisibleOutcome?.includes("用户可见行为通过完整验证"),
      ),
    ).toBe(true);
  });

  it("大范围迁移按 EXPAND、MIGRATE、CONTRACT 排序且 CONTRACT 依赖全部迁移", () => {
    const plan = new TddEngine().generatePlan({
      spec: threeRequirements.split("### Requirement: 取消订单")[0]!,
      design: "采用 expand-migrate-contract 完成 database migration",
      impact:
        "package.json\nREQ-001 创建订单: src/create-order.ts test/create-order.test.ts",
      codebaseSummary:
        "package.json\nsrc/create-order.ts\ntest/create-order.test.ts",
    });

    expect(plan.tasks.map((task) => task.sliceType)).toEqual([
      "EXPAND",
      "MIGRATE",
      "MIGRATE",
      "CONTRACT",
    ]);
    expect(plan.tasks[3]!.dependsOn).toEqual([
      "TASK-001-RED",
      "TASK-001-GREEN",
      "TASK-001-REFACTOR",
    ]);
  });

  it("文件范围不重叠时 requirement 链可以并行", () => {
    const plan = generate(explicitlyMappedImpact);

    expect(plan.tasks[4]!.dependsOn).toEqual([]);
    expect(plan.tasks[8]!.dependsOn).toEqual([]);
  });

  it("文件范围重叠时后一条链依赖前一条链 VERIFY", () => {
    const plan = generate(
      [
        "package.json",
        "REQ-001 创建订单: src/order.ts test/order.test.ts",
        "REQ-002 取消订单: src/order.ts test/order.test.ts",
        "REQ-003 查询订单: src/query-order.ts test/query-order.test.ts",
      ].join("\n"),
    );

    expect(plan.tasks[4]!.dependsOn).toEqual(["TASK-001-VERIFY"]);
  });

  it("后一条链同时重叠多个先前范围时依赖全部 VERIFY", () => {
    const spec = `# Spec
## ADDED Requirements
### Requirement: alpha
System SHALL support alpha.
#### Scenario: alpha works
- GIVEN alpha
- WHEN used
- THEN works
### Requirement: beta
System SHALL support beta.
#### Scenario: beta works
- GIVEN beta
- WHEN used
- THEN works
### Requirement: alpha beta
System SHALL combine alpha and beta.
#### Scenario: combination works
- GIVEN both
- WHEN used
- THEN works`;
    const plan = new TddEngine().generatePlan({
      spec,
      design: "src/alpha.ts src/beta.ts",
      impact:
        "package.json\nsrc/alpha.ts\ntest/alpha.test.ts\nsrc/beta.ts\ntest/beta.test.ts\nREQ-003: src/alpha.ts test/alpha.test.ts src/beta.ts test/beta.test.ts",
      codebaseSummary:
        "package.json\nsrc/alpha.ts\ntest/alpha.test.ts\nsrc/beta.ts\ntest/beta.test.ts",
    });

    expect(plan.tasks[4]!.dependsOn).toEqual([]);
    expect(plan.tasks[8]!.dependsOn).toEqual([
      "TASK-001-VERIFY",
      "TASK-002-VERIFY",
    ]);
  });

  it("支持安全的聚焦源码与测试目录并规范化为 glob", () => {
    const plan = new TddEngine().generatePlan({
      spec: threeRequirements.split("### Requirement: 取消订单")[0]!,
      design: "orders",
      impact: "package.json\nsrc/orders/\ntest/orders/",
      codebaseSummary: "package.json\nsrc/orders/\ntest/orders/",
    });

    expect(plan.tasks[0]!.allowedFiles).toEqual([
      "src/orders/**",
      "test/orders/**",
    ]);
  });

  it.each([
    ["根级宽泛目录", "package.json\nsrc/\ntest/"],
    ["当前目录", "package.json\n.\n./test/orders/"],
    ["绝对路径", "package.json\n/tmp/src/orders/\ntest/orders/"],
    ["上级路径", "package.json\nsrc/../orders/\ntest/orders/"],
  ])("拒绝%s范围", (_name, paths) => {
    expect(() =>
      new TddEngine().generatePlan({
        spec: threeRequirements.split("### Requirement: 取消订单")[0]!,
        design: "orders",
        impact: paths,
        codebaseSummary: paths,
      }),
    ).toThrowError(/精确的源码与测试文件范围/);
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

  it.each([
    ["没有 requirement", "# Empty"],
    [
      "requirement 没有 scenario",
      "# Spec\n## ADDED Requirements\n### Requirement: Empty\nSystem SHALL work.",
    ],
  ])("%s 时阻止计划", (_name, spec) => {
    expect(() =>
      new TddEngine().generatePlan({
        spec,
        design: "empty",
        impact: "package.json\nsrc/order.ts\ntest/order.test.ts",
        codebaseSummary: "package.json\nsrc/order.ts\ntest/order.test.ts",
      }),
    ).toThrowError(/Requirement|Scenario/);
  });

  it("候选摘要排序变化不影响显式映射", () => {
    const reversed = explicitlyMappedImpact.split("\n").reverse().join("\n");
    const first = generate(explicitlyMappedImpact);
    const second = generate(reversed, `package.json\n${reversed}`);

    expect(second.tasks.map((task) => task.allowedFiles)).toEqual(
      first.tasks.map((task) => task.allowedFiles),
    );
  });

  it("多候选无法唯一映射 requirement 时阻止计划", () => {
    expect(() =>
      generate(
        "package.json\nsrc/a.ts\ntest/a.test.ts\nsrc/b.ts\ntest/b.test.ts",
      ),
    ).toThrowError(/无法可靠关联/);
  });

  it("支持 Windows 相对路径并拒绝不安全 Windows 路径", () => {
    const plan = new TddEngine().generatePlan({
      spec: threeRequirements.split("### Requirement: 取消订单")[0]!,
      design: "order",
      impact:
        "package.json\nsrc\\orders\\order.ts\ntest\\orders\\order.test.ts",
      codebaseSummary:
        "package.json\nsrc\\orders\\order.ts\ntest\\orders\\order.test.ts",
    });
    expect(plan.tasks[0]!.allowedFiles).toEqual([
      "src/orders/order.ts",
      "test/orders/order.test.ts",
    ]);

    for (const unsafe of [
      "C:\\src\\order.ts",
      "\\\\server\\share\\order.ts",
      "src\\..\\order.ts",
      "\\src\\order.ts",
    ]) {
      expect(() =>
        new TddEngine().generatePlan({
          spec: threeRequirements.split("### Requirement: 取消订单")[0]!,
          design: "order",
          impact: `package.json\n${unsafe}\ntest/order.test.ts`,
          codebaseSummary: `package.json\n${unsafe}\ntest/order.test.ts`,
        }),
      ).toThrowError(/精确的源码与测试文件范围/);
    }
  });

  it("expectedNewFiles 仅包含明确标记为新增的路径", () => {
    const plan = new TddEngine().generatePlan({
      spec: threeRequirements.split("### Requirement: 取消订单")[0]!,
      design: "order",
      impact:
        "package.json\nREQ-001 src/order.ts 新增 src/order-helper.ts\ntest/order.test.ts (new)",
      codebaseSummary: "package.json\nsrc/order.ts\ntest/order.test.ts",
    });

    expect(plan.tasks[0]!.expectedNewFiles).toEqual([
      "src/order-helper.ts",
      "test/order.test.ts",
    ]);
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
      "TDD Instruction:",
    ])
      expect(plan.tasksMarkdown).toContain(heading);
    expect(plan.contextPacks["TASK-001-RED"]).toContain("Phase: RED");
    expect(plan.contextPacks["TASK-001-RED"]).toContain("## Depends On");
    expect(plan.contextPacks["TASK-001-RED"]).toContain(
      "## Expected New Files",
    );
    expect(plan.contextPacks["TASK-001-RED"]).toContain("REQ-001-SC-001");
    expect(plan.contextPacks["TASK-001-RED"]).toContain(
      "先写测试并观察其因目标行为缺失而预期失败",
    );
    expect(plan.tasksMarkdown).toContain(
      "TDD Instruction: 先写测试并观察其因目标行为缺失而预期失败。",
    );
    expect(plan.tasksMarkdown).toContain(
      "TDD Instruction: 编写最小实现使关联测试通过。",
    );
    expect(plan.tasksMarkdown).toContain(
      "TDD Instruction: 在重构过程中保持测试绿色。",
    );
    expect(plan.tasksMarkdown).toContain(
      "TDD Instruction: 运行完整验证命令并确认全部通过。",
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
