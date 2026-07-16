import type { PolicyBundle } from "@sdd-harness/agent-policies";
import { createAtomicTasks, extractPaths } from "../superpowers/planner.js";
import type {
  PlanArtifacts,
  PlanningInput,
  TaskDefinition,
  TddPhase,
} from "../superpowers/protocol.js";

export interface DesignInput {
  spec: string;
  impact: string;
  codebaseSummary: string;
  packageStructure: string;
  architecture: string;
  policyBundle?: PolicyBundle;
  existingDesign?: string;
}

type MaybePromise<T> = T | Promise<T>;

export type { PlanArtifacts, TaskDefinition, TddPhase };

export class TddEngine {
  generateDesign(input: DesignInput): MaybePromise<string> {
    const affectedFiles = extractPaths(
      `${input.impact}\n${input.codebaseSummary}\n${input.architecture}`,
    );
    const requirementLines = structuredRequirementLines(input.spec);
    let prompt = [
      "# Design",
      "",
      "## Phase Policy",
      "",
      input.policyBundle?.instructions ?? "",
      "",
      "## Current Code Structure",
      "",
      input.codebaseSummary,
      "",
      input.packageStructure,
      "",
      "## Structured Requirements and Scenarios",
      "",
      ...requirementLines,
      "",
      "## Target Design",
      "",
      "沿用已索引代码库的现有模块边界，以每个 Requirement 的 Scenario 作为可验证行为单元。",
      "",
      "## Affected Modules and Files",
      "",
      ...(affectedFiles.length === 0
        ? [input.architecture]
        : affectedFiles.map((file) => `- ${file}`)),
      "",
      input.architecture,
      "",
      "## API Changes",
      "",
      "仅公开规格明确要求的接口行为，并保持未涉及行为兼容。",
      "",
      "## Interfaces and Contracts",
      "",
      "模块间只通过上述公开接口交换规格所需数据；输入、输出与稳定错误均以 Scenario 为契约。",
      "",
      "## Data Changes",
      "",
      "仅持久化规格要求的状态；若涉及结构变更，需提供迁移和回滚验证。",
      "",
      "## Transaction and Idempotency",
      "",
      "状态修改保持原子性，并为规格中的重复操作定义稳定结果。",
      "",
      "## Error Handling",
      "",
      "按 Scenario 的失败路径返回稳定错误，不吞掉边界异常。",
      "",
      "## Logging and Monitoring",
      "",
      "记录必要状态变化，不记录密钥或完整源码内容。",
      "",
      "## Testing Strategy",
      "",
      "每个 Scenario 执行 RED、GREEN、REFACTOR、VERIFY 四阶段链。",
      "",
      "## Test Seams",
      "",
      "优先在公开 API 或模块导出边界建立稳定测试 seam，不依赖私有实现细节。",
      "",
      ...(input.policyBundle?.policies.some(
        ({ id }) => id === "design-it-twice",
      )
        ? [
            "## Alternative Comparison",
            "",
            "### 方案 A：沿用现有模块边界",
            "",
            "在现有接口内完成最小增量，迁移风险较低，但需接受现有模块约束。",
            "",
            "### 方案 B：新增隔离模块与适配接口",
            "",
            "边界更清晰且便于演进，但增加接口、迁移和回滚成本。",
            "",
            "### 决策",
            "",
            "默认选择方案 A；只有现有边界无法维持规格契约或回滚要求时才选择方案 B。",
            "",
          ]
        : []),
      "## Risks and Rollback",
      "",
      "风险由受影响文件、兼容边界和状态变更决定；代码与数据变更应可共同回滚。",
      "",
      "## Specification Reference",
      "",
      input.spec,
      "",
      "## Impact Reference",
      "",
      input.impact,
    ].join("\n");

    if (input.existingDesign !== undefined) {
      prompt += `

## 已有设计文档

以下是上次生成的设计文档。用户可能已在其上做了修改。请在已有内容基础上更新设计，遵循以下规则：
1. 保留用户新增或修改的内容（手动添加的需求分析、约束、取舍说明等）。
2. 仅因 spec/impact 变更而需要调整的部分才更新。
3. 输出完整的设计文档，不要输出 diff 或标记变更。`;
      prompt += `\n\n${input.existingDesign}`;
    }

    return prompt;
  }

  generatePlan(input: PlanningInput): MaybePromise<PlanArtifacts> {
    const { tasks, requirements } = createAtomicTasks(input);
    let context = [
      "# Change Context",
      "",
      "## Codebase",
      "",
      input.codebaseSummary,
      "",
      "## Impact",
      "",
      input.impact,
      "",
      "## Design",
      "",
      input.design,
      "",
      "## Phase Policy",
      "",
      input.policyBundle?.instructions ?? "",
    ].join("\n");
    if (input.existingPlan !== undefined) {
      context += `

## 已有计划制品

以下是上次生成的计划文件。用户可能已在其上做了修改。请在已有内容基础上更新，遵循以下规则：
1. 保留用户新增、删除或重排的任务。
2. 保留用户修改的测试计划和上下文。
3. 仅因设计变更而需要调整的部分才更新。
4. 输出完整的计划文件。`;
      context += `\n\n### 既有任务计划\n${input.existingPlan.tasksMarkdown}`;
      context += `\n\n### 既有测试计划\n${input.existingPlan.testPlan}`;
      context += `\n\n### 既有上下文摘要\n${input.existingPlan.context}`;
    }
    return {
      tasks,
      tasksMarkdown: renderTasks(tasks),
      testPlan: renderTestPlan(requirements),
      context,
      contextPacks: Object.fromEntries(
        tasks.map((task) => [task.id, renderContextPack(task)]),
      ),
      dependencies: input.existingPlan?.dependencies ?? [],
    };
  }
}

function renderTasks(tasks: TaskDefinition[]): string {
  return [
    "# Tasks",
    ...tasks.flatMap((task) => [
      "",
      `## ${task.id}: ${task.title}`,
      "",
      `Phase: ${task.phase}`,
      "",
      `Status: ${task.status}`,
      "",
      `TDD Instruction: ${phaseInstruction(task.phase)}`,
      "",
      ...list("Requirements", task.requirements),
      "",
      ...list("Scenarios", task.scenarios),
      "",
      ...list("Depends On", task.dependsOn),
      "",
      ...list("Allowed Files", task.allowedFiles),
      "",
      ...list("Expected New Files", task.expectedNewFiles),
      "",
      ...list("Forbidden Files", task.forbiddenFiles),
      "",
      ...list("Verification", task.verification),
      "",
      ...list("Done Criteria", task.doneCriteria),
    ]),
  ].join("\n");
}

function renderContextPack(task: TaskDefinition): string {
  return [
    `# Context Pack: ${task.id}`,
    "",
    "## Task",
    "",
    task.title,
    "",
    `Phase: ${task.phase}`,
    "",
    ...list("Requirements", task.requirements, 2),
    "",
    ...list("Scenarios", task.scenarios, 2),
    "",
    ...list("Depends On", task.dependsOn, 2),
    "",
    ...list("Expected New Files", task.expectedNewFiles, 2),
    "",
    "## TDD Instruction",
    "",
    phaseInstruction(task.phase),
    "",
    ...list("Allowed Files", task.allowedFiles, 2),
    "",
    ...list("Forbidden Files", task.forbiddenFiles, 2),
    "",
    "## Relevant Code Context",
    "",
    "按 Context Pack v2 References 中的 codebase 路径读取，不在此复制代码库摘要。",
    "",
    ...list("Verification", task.verification, 2),
    "",
    "## Risk",
    "",
    "不得扩大文件范围或绕过现有安全与架构边界。",
  ].join("\n");
}

function renderTestPlan(
  requirements: Array<{
    id: string;
    title: string;
    scenarios: Array<{ id: string; title: string }>;
  }>,
): string {
  return [
    "# Test Plan",
    ...requirements.flatMap((requirement) =>
      requirement.scenarios.flatMap((scenario) => [
        "",
        `## ${scenario.id}: ${scenario.title}`,
        "",
        `Requirement: ${requirement.id} ${requirement.title}`,
        "",
        "- RED：先实现能因目标行为缺失而失败的场景测试。",
        "- 正向路径：验证 Scenario 定义的成功结果。",
        "- 反向路径：验证前置条件不满足、无效输入或边界失败。",
        "- VERIFY：执行项目完整验证命令并保留结果。",
      ]),
    ),
  ].join("\n");
}

function phaseInstruction(phase: TddPhase): string {
  return {
    RED: "先写测试并观察其因目标行为缺失而预期失败。",
    GREEN: "编写最小实现使关联测试通过。",
    REFACTOR: "在重构过程中保持测试绿色。",
    VERIFY: "运行完整验证命令并确认全部通过。",
  }[phase];
}

function list(title: string, values: string[], level = 0): string[] {
  const heading = level === 0 ? `${title}:` : `${"#".repeat(level)} ${title}`;
  return [
    heading,
    ...(values.length === 0 ? ["- None"] : values.map((value) => `- ${value}`)),
  ];
}

function structuredRequirementLines(spec: string): string[] {
  const lines = spec
    .split(/\r?\n/)
    .filter((line) => /^### (?:Requirement:|REQ-)|^#### Scenario:/.test(line));
  return lines.length === 0
    ? ["- 规格未包含结构化 Requirement。"]
    : lines.map((line) => `- ${line.replace(/^#+\s*/, "")}`);
}
