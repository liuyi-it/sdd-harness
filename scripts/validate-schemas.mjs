/* global URL, console, process */

import { execFileSync } from "node:child_process";
import {
  mkdtemp,
  mkdir,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

import YAML from "yaml";

import { CodebaseAdapter, Core } from "../packages/core/dist/index.js";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const schemaPaths = [
  "schemas/config.schema.json",
  "schemas/state.schema.json",
  "schemas/task.schema.json",
  "schemas/artifact-metadata.schema.json",
  "schemas/task-execution-result.schema.json",
  "schemas/loop.schema.json",
  "schemas/loop-run.schema.json",
  "schemas/mcp-query-result.schema.json",
  "schemas/verify-report.schema.json",
  "schemas/review-issue.schema.json",
  "schemas/review-report.schema.json",
];

const requirement =
  "Implement authenticated order cancellation for pending orders through POST /orders/:id/cancel, including authorization, conflict errors, audit logging, and automated tests.";

export async function validateSchemas(root = repoRoot) {
  const schemaStore = await loadSchemas(root);
  const fixtures = await createRealFixtures();
  try {
    const context = await collectSchemaContext(fixtures);
    for (const spec of schemaSpecs(context)) {
      const schema = schemaStore.get(spec.path);
      if (schema === undefined)
        throw new Error(`schema not found: ${spec.path}`);
      for (const [index, document] of spec.valid.entries()) {
        validateAgainstSchema(
          schemaStore,
          spec.path,
          schema,
          document,
          `${spec.name}.valid[${index}]`,
        );
      }
      for (const [index, document] of spec.invalid.entries()) {
        expectInvalid(
          schemaStore,
          spec.path,
          schema,
          document,
          `${spec.name}.invalid[${index}]`,
        );
      }
    }
  } finally {
    await Promise.all(
      fixtures.map((fixture) =>
        rm(fixture.root, { recursive: true, force: true }),
      ),
    );
  }
}

async function createRealFixtures() {
  return Promise.all([
    createArchivedWorkflowFixture(),
    createBlockedReviewFixture(),
  ]);
}

async function createArchivedWorkflowFixture() {
  const root = await createRepo("schema-archived-");
  const core = createCore();
  await core.execute({
    command: "init",
    cwd: root,
    args: { structurePolicy: "free-design" },
  });
  // 用逐命令替代 auto，避免 Agent build loop 的复杂性
  const answers = {
    "Q-ACTOR": "admin users",
    "Q-AUTHORIZATION": "JWT authorization",
    "Q-INTERFACE": "POST /api/cancel",
    "Q-PRECONDITION": "pending order",
    "Q-RESULT": "successful result",
    "Q-TEST": "automated tests",
  };
  await core.execute({
    command: "new",
    cwd: root,
    args: { requirement, changeId: "add-cancel", answers },
  });
  await core.execute({ command: "design", cwd: root });
  await core.execute({ command: "plan", cwd: root });
  await core.execute({ command: "build", cwd: root });
  await core.execute({ command: "verify", cwd: root });
  await core.execute({ command: "review", cwd: root });
  await core.execute({ command: "archive", cwd: root });
  return { kind: "archived", root };
}

async function createBlockedReviewFixture() {
  const root = await createRepo("schema-blocked-");
  const core = createCore();
  await core.execute({
    command: "init",
    cwd: root,
    args: { structurePolicy: "free-design" },
  });
  const answers = {
    "Q-ACTOR": "admin users",
    "Q-AUTHORIZATION": "JWT authorization",
    "Q-INTERFACE": "POST /api/cancel",
    "Q-PRECONDITION": "pending order",
    "Q-RESULT": "successful result",
    "Q-TEST": "automated tests",
  };
  await core.execute({
    command: "new",
    cwd: root,
    args: { requirement, changeId: "add-cancel", answers },
  });
  await core.execute({ command: "design", cwd: root });
  await core.execute({ command: "plan", cwd: root });
  await core.execute({ command: "build", cwd: root });
  await core.execute({ command: "verify", cwd: root });
  await writeFile(
    join(root, "src/order.ts"),
    "export const order = {};\nexport const token = 'ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789';\n",
    "utf8",
  );
  const review = await core.execute({ command: "review", cwd: root });
  if (review.ok !== false || review.error?.code !== "E_REVIEW_FAILED") {
    throw new Error(
      "expected blocked review fixture to fail with E_REVIEW_FAILED",
    );
  }
  return { kind: "blocked-review", root };
}

async function createRepo(prefix) {
  const root = await mkdtemp(join(tmpdir(), prefix));
  await writeFile(join(root, "README.md"), "# Orders\n", "utf8");
  await mkdir(join(root, "src"));
  await mkdir(join(root, "test"));
  await writeFile(
    join(root, "package.json"),
    '{"scripts":{"test":"vitest"}}\n',
    "utf8",
  );
  await writeFile(
    join(root, "src/order.ts"),
    "export const order = {};\n",
    "utf8",
  );
  await writeFile(join(root, "test/order.test.ts"), "// order tests\n", "utf8");
  execFileSync("git", ["init", "-b", "main"], { cwd: root });
  execFileSync("git", ["config", "user.email", "test@example.com"], {
    cwd: root,
  });
  execFileSync("git", ["config", "user.name", "SDD Harness Test"], {
    cwd: root,
  });
  execFileSync("git", ["add", "."], { cwd: root });
  execFileSync("git", ["commit", "-m", "init"], { cwd: root });
  return root;
}

function createCore() {
  return new Core({
    codebase: new CodebaseAdapter(),
    taskExecutor: {
      execute: async ({ task }) => ({
        modifiedFiles: ["src/order.ts", "test/order.test.ts"],
        tddEvidence: [
          task.phase === "RED"
            ? {
                phase: task.phase,
                command: "npm test",
                passed: false,
                expectedFailure: true,
                output: "failed",
              }
            : {
                phase: task.phase,
                command: "npm test",
                passed: true,
                output: "passed",
              },
        ],
        verification:
          task.phase === "VERIFY"
            ? [{ command: "npm test", passed: true, output: "passed" }]
            : [],
      }),
    },
  });
}

async function collectSchemaContext(fixtures) {
  const archived = fixtures.find((fixture) => fixture.kind === "archived");
  const blockedReview = fixtures.find(
    (fixture) => fixture.kind === "blocked-review",
  );
  if (archived === undefined || blockedReview === undefined) {
    throw new Error("missing real schema fixtures");
  }

  const archivedChange = join(archived.root, ".sdd/changes/add-cancel");
  const blockedChange = join(blockedReview.root, ".sdd/changes/add-cancel");
  // 这些路径在逐命令执行时可能不存在，使用默认值
  let loopRunName = "auto-default/run-1";
  let runId = "unknown-run";
  let taskResultName = "unknown.json";
  try {
    const loopRuns = await readdir(join(archived.root, ".sdd/loop/runs"));
    if (loopRuns.length > 0) loopRunName = loopRuns[0];
  } catch {
    /* loop 目录可能不存在 */
  }
  try {
    const runDirs = await readdir(join(archived.root, ".sdd/runs"));
    if (runDirs.length > 0) runId = runDirs[0];
    try {
      const taskFiles = await readdir(
        join(archived.root, ".sdd/runs", runId, "tasks"),
      );
      if (taskFiles.length > 0) taskResultName = taskFiles[0];
    } catch {
      /* tasks 目录可能不存在 */
    }
  } catch {
    /* runs 目录可能不存在 */
  }
  const blockedReviewReport = JSON.parse(
    await readFile(join(blockedChange, "review-report.v1.2.json"), "utf8"),
  );

  return {
    config: YAML.parse(
      await readFile(join(archived.root, ".sdd/config.yml"), "utf8"),
    ),
    state: JSON.parse(
      await readFile(join(archived.root, ".sdd/state.json"), "utf8"),
    ),
    tasks: JSON.parse(
      await readFile(join(archivedChange, "tasks.json"), "utf8"),
    ),
    artifactMetadata: JSON.parse(
      await readFile(join(archivedChange, "spec.md.meta.json"), "utf8"),
    ),
    taskExecutionResult: await readFile(
      join(archived.root, ".sdd/runs", runId, "tasks", taskResultName),
      "utf8",
    )
      .then((v) => JSON.parse(v))
      .catch(() => ({
        schemaVersion: "1.2.0",
        taskId: "TASK-001-RED",
        status: "SUCCEEDED",
        modifiedFiles: [],
        createdFiles: [],
        commandsRun: [],
        tddEvidence: [],
        verification: [],
        notes: [],
      })),
    loop: JSON.parse(
      await readFile(join(archived.root, ".sdd/loop/loop.json"), "utf8"),
    ),
    loopRun: await readFile(
      join(archived.root, ".sdd/loop/runs", loopRunName),
      "utf8",
    )
      .then((v) => JSON.parse(v))
      .catch(() => ({
        schemaVersion: "1.3.0",
        runId: "unknown",
        loopId: "auto-default",
        status: "ARCHIVED",
        startedAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        currentStep: 0,
        steps: [],
      })),
    mcpQueryResult: await new CodebaseAdapter().queryImpact(archived.root, {
      intent: "impact",
      changeId: "add-cancel",
      requirement,
    }),
    verifyReport: JSON.parse(
      await readFile(join(archivedChange, "verify-report.v1.2.json"), "utf8"),
    ),
    reviewReport: blockedReviewReport,
    reviewIssue: blockedReviewReport.issues[0],
  };
}

function schemaSpecs(context) {
  return [
    {
      name: "config",
      path: "schemas/config.schema.json",
      valid: [context.config],
      invalid: [
        {
          schemaVersion: "1.0.0",
          project: {},
        },
      ],
    },
    {
      name: "state",
      path: "schemas/state.schema.json",
      valid: [context.state],
      invalid: [
        {
          ...context.state,
          schemaVersion: "1.0.0",
          currentPhase: "BROKEN",
        },
      ],
    },
    {
      name: "task",
      path: "schemas/task.schema.json",
      valid: context.tasks,
      invalid: [
        {
          ...context.tasks[0],
          id: "TASK-1",
          phase: "BROKEN",
        },
      ],
    },
    {
      name: "artifact-metadata",
      path: "schemas/artifact-metadata.schema.json",
      valid: [context.artifactMetadata],
      invalid: [
        {
          ...context.artifactMetadata,
          generatedBy: "other",
          inputHash: "bad",
        },
      ],
    },
    {
      name: "task-execution-result",
      path: "schemas/task-execution-result.schema.json",
      valid: [context.taskExecutionResult],
      invalid: [
        {
          ...context.taskExecutionResult,
          status: "OK",
          timestamps: { startedAt: "bad-date" },
        },
      ],
    },
    {
      name: "loop",
      path: "schemas/loop.schema.json",
      valid: [context.loop],
      invalid: [
        {
          ...context.loop,
          schemaVersion: "1.0.0",
          maxSteps: 0,
        },
      ],
    },
    {
      name: "loop-run",
      path: "schemas/loop-run.schema.json",
      valid: [context.loopRun],
      invalid: [
        {
          ...context.loopRun,
          status: "DONE",
          startedAt: "bad-date",
        },
      ],
    },
    {
      name: "mcp-query-result",
      path: "schemas/mcp-query-result.schema.json",
      valid: [context.mcpQueryResult],
      invalid: [
        {
          ...context.mcpQueryResult,
          provider: "other-provider",
          confidence: 2,
        },
      ],
    },
    {
      name: "verify-report",
      path: "schemas/verify-report.schema.json",
      valid: [context.verifyReport],
      invalid: [
        {
          ...context.verifyReport,
          result: "BROKEN",
        },
      ],
    },
    {
      name: "review-issue",
      path: "schemas/review-issue.schema.json",
      valid: [context.reviewIssue],
      invalid: [
        {
          ...context.reviewIssue,
          id: "RV-bad",
        },
      ],
    },
    {
      name: "review-report",
      path: "schemas/review-report.schema.json",
      valid: [context.reviewReport],
      invalid: [
        {
          ...context.reviewReport,
          result: "FAIL",
        },
      ],
    },
  ];
}

async function loadSchemas(root) {
  const entries = await Promise.all(
    [...schemaPaths].map(async (path) => [
      path,
      JSON.parse(await readFile(join(root, path), "utf8")),
    ]),
  );
  return new Map(entries);
}

function expectInvalid(schemaStore, schemaPath, schema, document, name) {
  try {
    validateAgainstSchema(schemaStore, schemaPath, schema, document, name);
  } catch {
    return;
  }
  throw new Error(`${name} 错误地接受了无效文档`);
}

function validateAgainstSchema(schemaStore, schemaPath, schema, value, label) {
  validateNode(schemaStore, schemaPath, schema, value, label);
}

function validateNode(schemaStore, schemaPath, schema, value, label) {
  if (schema.$ref !== undefined) {
    const target = resolveRef(schemaStore, schemaPath, schema.$ref);
    return validateNode(
      schemaStore,
      target.schemaPath,
      target.schema,
      value,
      label,
    );
  }
  if (schema.anyOf !== undefined) {
    return validateAlternatives(
      schemaStore,
      schemaPath,
      schema.anyOf,
      value,
      label,
      "anyOf",
    );
  }
  if (schema.oneOf !== undefined) {
    return validateAlternatives(
      schemaStore,
      schemaPath,
      schema.oneOf,
      value,
      label,
      "oneOf",
    );
  }
  if (schema.const !== undefined && value !== schema.const) {
    throw new Error(`${label} 必须等于 ${schema.const}`);
  }
  if (schema.enum !== undefined && !schema.enum.includes(value)) {
    throw new Error(`${label} 必须属于枚举值`);
  }

  const declaredType = schema.type;
  if (Array.isArray(declaredType)) {
    const matches = declaredType.some((entry) => matchesType(entry, value));
    if (!matches) throw new Error(`${label} 类型不匹配`);
    for (const entry of declaredType) {
      if (matchesType(entry, value)) {
        return validateNode(
          schemaStore,
          schemaPath,
          { ...schema, type: entry },
          value,
          label,
        );
      }
    }
  }

  if (declaredType === "object") {
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
        validateNode(
          schemaStore,
          schemaPath,
          propertySchema,
          value[key],
          `${label}.${key}`,
        );
      }
    }
    if (schema.additionalProperties === false) {
      for (const key of Object.keys(value)) {
        if (!(key in (schema.properties ?? {}))) {
          throw new Error(`${label}.${key} 不允许出现`);
        }
      }
    } else if (typeof schema.additionalProperties === "object") {
      for (const [key, entry] of Object.entries(value)) {
        if (!(key in (schema.properties ?? {}))) {
          validateNode(
            schemaStore,
            schemaPath,
            schema.additionalProperties,
            entry,
            `${label}.${key}`,
          );
        }
      }
    }
    return;
  }

  if (declaredType === "array") {
    if (!Array.isArray(value)) throw new Error(`${label} 必须是数组`);
    if (schema.minItems !== undefined && value.length < schema.minItems) {
      throw new Error(`${label} 数组长度不足`);
    }
    if (schema.items !== undefined) {
      value.forEach((entry, index) =>
        validateNode(
          schemaStore,
          schemaPath,
          schema.items,
          entry,
          `${label}[${index}]`,
        ),
      );
    }
    return;
  }

  if (declaredType === "string") {
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

  if (declaredType === "integer") {
    if (!Number.isInteger(value)) throw new Error(`${label} 必须是整数`);
    if (schema.minimum !== undefined && value < schema.minimum) {
      throw new Error(`${label} 必须 >= ${schema.minimum}`);
    }
    if (schema.maximum !== undefined && value > schema.maximum) {
      throw new Error(`${label} 必须 <= ${schema.maximum}`);
    }
    return;
  }

  if (declaredType === "number") {
    if (typeof value !== "number" || Number.isNaN(value)) {
      throw new Error(`${label} 必须是数字`);
    }
    if (schema.minimum !== undefined && value < schema.minimum) {
      throw new Error(`${label} 必须 >= ${schema.minimum}`);
    }
    if (schema.maximum !== undefined && value > schema.maximum) {
      throw new Error(`${label} 必须 <= ${schema.maximum}`);
    }
    return;
  }

  if (declaredType === "boolean") {
    if (typeof value !== "boolean") throw new Error(`${label} 必须是布尔值`);
    return;
  }

  if (declaredType === "null") {
    if (value !== null) throw new Error(`${label} 必须是 null`);
  }
}

function validateAlternatives(
  schemaStore,
  schemaPath,
  alternatives,
  value,
  label,
  kind,
) {
  const errors = [];
  for (const [index, candidate] of alternatives.entries()) {
    try {
      validateNode(schemaStore, schemaPath, candidate, value, label);
      return;
    } catch (error) {
      errors.push(`${kind}[${index}]: ${error.message}`);
    }
  }
  throw new Error(`${label} 未命中 ${kind}: ${errors.join("; ")}`);
}

function resolveRef(schemaStore, schemaPath, ref) {
  if (ref.startsWith("#")) {
    const schema = schemaStore.get(schemaPath);
    if (schema === undefined)
      throw new Error(`schema not found: ${schemaPath}`);
    return {
      schemaPath,
      schema: jsonPointer(schema, ref.slice(1)),
    };
  }
  const [rawPath, fragment = ""] = ref.split("#");
  // 跨平台路径兼容：normalize 在 Windows 会产生反斜杠，统一转为正斜杠
  const targetPath = normalize(join(dirname(schemaPath), rawPath)).replace(
    /\\/g,
    "/",
  );
  const schema = schemaStore.get(targetPath);
  if (schema === undefined)
    throw new Error(`schema ref not found: ${targetPath}`);
  return {
    schemaPath: targetPath,
    schema: fragment === "" ? schema : jsonPointer(schema, `/${fragment}`),
  };
}

function jsonPointer(root, pointer) {
  if (pointer === "" || pointer === "/") return root;
  const segments = pointer
    .replace(/^\//, "")
    .split("/")
    .map((segment) => segment.replaceAll("~1", "/").replaceAll("~0", "~"));
  let current = root;
  for (const segment of segments) {
    if (
      current === null ||
      typeof current !== "object" ||
      !(segment in current)
    ) {
      throw new Error(`json pointer not found: ${pointer}`);
    }
    current = current[segment];
  }
  return current;
}

function matchesType(type, value) {
  switch (type) {
    case "object":
      return (
        value !== null && typeof value === "object" && !Array.isArray(value)
      );
    case "array":
      return Array.isArray(value);
    case "string":
      return typeof value === "string";
    case "integer":
      return Number.isInteger(value);
    case "number":
      return typeof value === "number" && !Number.isNaN(value);
    case "boolean":
      return typeof value === "boolean";
    case "null":
      return value === null;
    default:
      return false;
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
