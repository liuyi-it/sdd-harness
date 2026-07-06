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

function validateNode(schema: any, value: any, label: string): void {
  if (schema.const !== undefined && value !== schema.const) {
    throw new Error(`${label} 必须等于 ${schema.const}`);
  }
  if (schema.enum !== undefined && !schema.enum.includes(value)) {
    throw new Error(`${label} 必须属于枚举值`);
  }
  if (schema.type === "object") {
    if (value === null || typeof value !== "object" || Array.isArray(value)) {
      throw new Error(`${label} 必须是对象`);
    }
    for (const key of schema.required ?? []) {
      if (!(key in value)) throw new Error(`${label}.${key} 缺失`);
    }
    for (const [key, propertySchema] of Object.entries(
      schema.properties ?? {},
    )) {
      if (key in value) {
        validateNode(propertySchema, value[key], `${label}.${key}`);
      }
    }
    if (schema.additionalProperties === false) {
      for (const key of Object.keys(value)) {
        if (!(key in (schema.properties ?? {}))) {
          throw new Error(`${label}.${key} 不允许出现`);
        }
      }
    }
    if (
      schema.additionalProperties &&
      typeof schema.additionalProperties === "object"
    ) {
      for (const [key, entry] of Object.entries(value)) {
        if (!(key in (schema.properties ?? {}))) {
          validateNode(schema.additionalProperties, entry, `${label}.${key}`);
        }
      }
    }
    return;
  }
  if (schema.type === "array") {
    if (!Array.isArray(value)) throw new Error(`${label} 必须是数组`);
    if (schema.minItems !== undefined && value.length < schema.minItems) {
      throw new Error(`${label} 数组长度不足`);
    }
    if (schema.items !== undefined) {
      value.forEach((entry, index) =>
        validateNode(schema.items, entry, `${label}[${index}]`),
      );
    }
    return;
  }
  if (schema.type === "string") {
    if (typeof value !== "string") throw new Error(`${label} 必须是字符串`);
    if (schema.minLength !== undefined && value.length < schema.minLength) {
      throw new Error(`${label} 长度不足`);
    }
    if (
      schema.pattern !== undefined &&
      !new RegExp(schema.pattern).test(value)
    ) {
      throw new Error(`${label} 不匹配 ${schema.pattern}`);
    }
    if (schema.format === "date-time" && Number.isNaN(Date.parse(value))) {
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
