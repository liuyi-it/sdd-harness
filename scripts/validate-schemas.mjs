/* global URL, console, process */

import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));

const schemaSpecs = [
  {
    name: "config",
    path: "schemas/config.schema.json",
    valid: {
      schemaVersion: "1.0.0",
      project: { name: "demo" },
      plugins: { claudeCode: { enabled: true } },
      codebase: { provider: "codebase-memory-mcp" },
      workflow: { stopOnFailure: true },
      quality: { requireFileScopeCheck: true },
      security: { blockOutsideRepo: true },
    },
    invalid: {
      schemaVersion: "1.0.0",
      project: {},
    },
  },
  {
    name: "state",
    path: "schemas/state.schema.json",
    valid: {
      schemaVersion: "1.0.0",
      version: 1,
      updatedAt: new Date().toISOString(),
      initialized: true,
      currentPhase: "INDEX_READY",
      indexStatus: "INDEX_READY",
      tasks: {},
      artifacts: {},
    },
    invalid: {
      schemaVersion: "1.0.0",
      version: 0,
      updatedAt: "bad-date",
      initialized: true,
      currentPhase: "BROKEN",
      indexStatus: "INDEX_READY",
      tasks: {},
      artifacts: {},
    },
  },
  {
    name: "task",
    path: "schemas/task.schema.json",
    valid: {
      id: "TASK-001",
      title: "实现功能",
      status: "PENDING",
      requirements: ["REQ-001"],
      dependsOn: [],
      allowedFiles: ["src/index.ts"],
      verification: ["npm test"],
      doneCriteria: ["测试通过"],
    },
    invalid: {
      id: "TASK-1",
      title: "",
      status: "BROKEN",
      requirements: [],
      dependsOn: [],
      allowedFiles: [],
      verification: [],
      doneCriteria: [],
    },
  },
  {
    name: "artifact-metadata",
    path: "schemas/artifact-metadata.schema.json",
    valid: {
      schemaVersion: "1.0.0",
      generatedBy: "sdd-harness",
      inputHash: `sha256:${"a".repeat(64)}`,
      artifactHash: `sha256:${"b".repeat(64)}`,
      createdAt: new Date().toISOString(),
    },
    invalid: {
      schemaVersion: "1.0.0",
      generatedBy: "other",
      inputHash: "bad",
      artifactHash: "bad",
      createdAt: "not-a-date",
    },
  },
];

export async function validateSchemas(root = repoRoot) {
  for (const spec of schemaSpecs) {
    const schema = JSON.parse(await readFile(join(root, spec.path), "utf8"));
    validateAgainstSchema(schema, spec.valid, spec.name);
    expectInvalid(schema, spec.invalid, spec.name);
  }
}

function expectInvalid(schema, document, name) {
  try {
    validateAgainstSchema(schema, document, name);
  } catch {
    return;
  }
  throw new Error(`${name} schema 错误地接受了无效文档`);
}

function validateAgainstSchema(schema, value, label) {
  validateNode(schema, value, label);
}

function validateNode(schema, value, label) {
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
      if (key in value)
        validateNode(propertySchema, value[key], `${label}.${key}`);
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
  if (schema.type === "boolean") {
    if (typeof value !== "boolean") throw new Error(`${label} 必须是布尔值`);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  validateSchemas()
    .then(() => {
      console.log("schema validation passed");
    })
    .catch((error) => {
      console.error(error.message);
      process.exitCode = 1;
    });
}
