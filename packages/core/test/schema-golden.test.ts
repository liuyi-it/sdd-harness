import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { TddEngine } from "../src/engines/tdd/tdd-engine.js";

const rootDir = fileURLToPath(new URL("../../..", import.meta.url));

describe("Schema 1.2.0", () => {
  it("accepts a real TDD task with phase and scenarios", async () => {
    const schema = await loadSchema("schemas/task.schema.json");
    const task = createRealTask();

    expect(() => validateNode(schema, task, "task")).not.toThrow();
  });

  it("rejects a 1.2.0 task when phase or scenarios are missing", async () => {
    const schema = await loadSchema("schemas/task.schema.json");
    const task = createRealTask();

    const missingPhase = { ...task };
    delete missingPhase.phase;

    const missingScenarios = { ...task };
    delete missingScenarios.scenarios;

    expect(() => validateNode(schema, missingPhase, "task")).toThrow(
      "task.phase 缺失",
    );
    expect(() => validateNode(schema, missingScenarios, "task")).toThrow(
      "task.scenarios 缺失",
    );
  });

  it("validates the new 1.2.0 loop and execution result schemas", async () => {
    const taskExecutionResultSchema = await loadSchema(
      "schemas/task-execution-result.schema.json",
    );
    const loopSchema = await loadSchema("schemas/loop.schema.json");
    const loopRunSchema = await loadSchema("schemas/loop-run.schema.json");

    expect(() =>
      validateNode(
        taskExecutionResultSchema,
        validTaskExecutionResult(),
        "result",
      ),
    ).not.toThrow();
    expect(() =>
      validateNode(loopSchema, validLoopSpec(), "loop"),
    ).not.toThrow();
    expect(() =>
      validateNode(loopRunSchema, validLoopRun(), "loopRun"),
    ).not.toThrow();
  });
});

async function loadSchema(
  relativePath: string,
): Promise<Record<string, unknown>> {
  return JSON.parse(await readFile(join(rootDir, relativePath), "utf8"));
}

function createRealTask() {
  const engine = new TddEngine();
  const plan = engine.generatePlan({
    spec: [
      "# Spec",
      "",
      "## ADDED Requirements",
      "",
      "### Requirement: 允许用户继续运行",
      "系统 SHALL 在存在活动运行时继续当前 Loop。",
      "",
      "#### Scenario: 恢复已有运行",
      "",
      "- GIVEN 已存在运行",
      "- WHEN 用户执行 auto",
      "- THEN 系统继续当前运行",
    ].join("\n"),
    design: "# Design\n",
    impact: [
      "package.json",
      "REQ-001 允许用户继续运行: packages/core/src/commands/auto.ts packages/core/test/auto.test.ts",
    ].join("\n"),
    codebaseSummary: [
      "package.json",
      "packages/core/src/commands/auto.ts",
      "packages/core/test/auto.test.ts",
    ].join("\n"),
  });

  return plan.tasks[0]!;
}

function validTaskExecutionResult() {
  return {
    schemaVersion: "1.2.0",
    taskId: "TASK-001-RED",
    status: "SUCCEEDED",
    summary: "RED 阶段已完成",
    commandEvidence: [
      {
        command: "npm",
        args: ["test", "--", "packages/core/test/auto.test.ts"],
        exitCode: 1,
        outputSummary: "预期失败",
      },
    ],
    fileDelta: {
      added: ["packages/core/test/auto.test.ts"],
      modified: [],
      deleted: [],
    },
    timestamps: {
      startedAt: new Date().toISOString(),
      endedAt: new Date().toISOString(),
    },
  };
}

function validLoopSpec() {
  return {
    schemaVersion: "1.2.0",
    loopId: "auto-default",
    mode: "auto",
    maxSteps: 12,
    stoppingRules: ["VERIFY_FAILED", "REVIEW_FAILED", "HUMAN_CLARIFICATION"],
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  };
}

function validLoopRun() {
  return {
    schemaVersion: "1.2.0",
    runId: "RUN-20260706-001",
    loopId: "auto-default",
    status: "RUNNING",
    startedAt: new Date().toISOString(),
    steps: [
      {
        step: 1,
        command: "plan",
        status: "SUCCEEDED",
        startedAt: new Date().toISOString(),
        endedAt: new Date().toISOString(),
      },
    ],
  };
}

type JsonNode = Record<string, unknown>;

function validateNode(schema: unknown, value: unknown, label: string): void {
  const node = schema as JsonNode;
  if (node.const !== undefined && value !== node.const) {
    throw new Error(`${label} 必须等于 ${String(node.const)}`);
  }
  if (Array.isArray(node.enum) && !node.enum.includes(value)) {
    throw new Error(`${label} 必须属于枚举值`);
  }
  if (node.type === "object") {
    if (value === null || typeof value !== "object" || Array.isArray(value)) {
      throw new Error(`${label} 必须是对象`);
    }
    for (const key of (node.required as string[] | undefined) ?? []) {
      if (!(key in value)) throw new Error(`${label}.${key} 缺失`);
    }
    for (const [key, propertySchema] of Object.entries(
      (node.properties as Record<string, unknown> | undefined) ?? {},
    )) {
      if (key in (value as JsonNode)) {
        validateNode(
          propertySchema,
          (value as JsonNode)[key],
          `${label}.${key}`,
        );
      }
    }
    if (node.additionalProperties === false) {
      for (const key of Object.keys(value)) {
        if (
          !(
            key in
            ((node.properties as Record<string, unknown> | undefined) ?? {})
          )
        ) {
          throw new Error(`${label}.${key} 不允许出现`);
        }
      }
    }
    if (
      node.additionalProperties &&
      typeof node.additionalProperties === "object"
    ) {
      for (const [key, entry] of Object.entries(value)) {
        if (
          !(
            key in
            ((node.properties as Record<string, unknown> | undefined) ?? {})
          )
        ) {
          validateNode(node.additionalProperties, entry, `${label}.${key}`);
        }
      }
    }
    return;
  }
  if (node.type === "array") {
    if (!Array.isArray(value)) throw new Error(`${label} 必须是数组`);
    if (typeof node.minItems === "number" && value.length < node.minItems) {
      throw new Error(`${label} 数组长度不足`);
    }
    if (node.items !== undefined) {
      value.forEach((entry, index) =>
        validateNode(node.items, entry, `${label}[${index}]`),
      );
    }
    return;
  }
  if (node.type === "string") {
    if (typeof value !== "string") throw new Error(`${label} 必须是字符串`);
    if (typeof node.minLength === "number" && value.length < node.minLength) {
      throw new Error(`${label} 长度不足`);
    }
    if (
      typeof node.pattern === "string" &&
      !new RegExp(node.pattern).test(value)
    ) {
      throw new Error(`${label} 不匹配 ${node.pattern}`);
    }
    if (node.format === "date-time" && Number.isNaN(Date.parse(value))) {
      throw new Error(`${label} 不是合法日期时间`);
    }
    return;
  }
  if (schema.type === "integer") {
    if (!Number.isInteger(value)) throw new Error(`${label} 必须是整数`);
    if (schema.minimum !== undefined && value < schema.minimum) {
      throw new Error(`${label} 必须 >= ${schema.minimum}`);
    }
    return;
  }
}
