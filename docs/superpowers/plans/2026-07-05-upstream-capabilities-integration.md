# OpenSpec 与 Superpowers 上游能力内置 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将固定版本 OpenSpec 与 Superpowers 的完整上游快照和关键工作流语义内置到 Core，使 `.sdd/` 制品具备结构化规格、可执行原子任务、TDD 证据和端到端追踪能力。

**Architecture:** `vendor/*/upstream` 保存不可自动漂移的完整源码快照，`packages/core/src/engines/openspec` 和 `packages/core/src/engines/superpowers` 提供稳定内部模型与适配器。现有命令继续拥有状态、锁、文件和恢复职责；Claude Code/Codex Adapter 不直接依赖上游代码，最终事实源仍是 `.sdd/`。

**Tech Stack:** TypeScript 5.9、Node.js 20+、Vitest、Zod、npm workspaces、GitHub Actions。

---

### Task 1: 固定并校验完整上游快照

**Files:**

- Create: `vendor/openspec/upstream/**`
- Create: `vendor/superpowers/upstream/**`
- Create: `vendor/openspec/VERSION.json`
- Create: `vendor/superpowers/VERSION.json`
- Create: `scripts/vendor-manifest.mjs`
- Create: `vendor/openspec/MANIFEST.sha256`
- Create: `vendor/superpowers/MANIFEST.sha256`
- Modify: `THIRD_PARTY_NOTICES.md`
- Modify: `scripts/validate-release.mjs`
- Test: `packages/core/test/dependency-metadata.test.ts`
- Test: `packages/adapters-test/release-validation.test.ts`

- [ ] **Step 1: 写入会失败的快照完整性测试**

```ts
it("固定完整 OpenSpec 与 Superpowers 上游快照", async () => {
  for (const dependency of [
    ["openspec", "v1.4.1", "1b06fddd59d8e592d5b5794a1970b22867e85b1f"],
    ["superpowers", "v6.1.1", "d884ae04edebef577e82ff7c4e143debd0bbec99"],
  ] as const) {
    const [name, version, commit] = dependency;
    const metadata = JSON.parse(
      await readFile(join(root, "vendor", name, "VERSION.json"), "utf8"),
    );
    expect(metadata).toMatchObject({ name, version, commit, license: "MIT" });
    expect(
      await readFile(join(root, "vendor", name, "MANIFEST.sha256"), "utf8"),
    ).toContain("upstream/LICENSE");
  }
});
```

- [ ] **Step 2: 运行测试并确认 RED**

Run: `npx vitest run packages/core/test/dependency-metadata.test.ts`

Expected: FAIL，提示 `VERSION.json` 或 `MANIFEST.sha256` 不存在。

- [ ] **Step 3: 获取固定提交并生成确定性清单**

```bash
git clone https://github.com/Fission-AI/OpenSpec.git /private/tmp/openspec-v1.4.1
git -C /private/tmp/openspec-v1.4.1 checkout 1b06fddd59d8e592d5b5794a1970b22867e85b1f
git clone https://github.com/obra/superpowers.git /private/tmp/superpowers-v6.1.1
git -C /private/tmp/superpowers-v6.1.1 checkout d884ae04edebef577e82ff7c4e143debd0bbec99
```

复制时排除上游 `.git/`，保留其余跟踪文件。`scripts/vendor-manifest.mjs` 必须递归排序相对路径并输出 `<sha256>  upstream/<path>`；忽略 `MANIFEST.sha256` 自身。

`VERSION.json` 固定结构：

```json
{
  "name": "openspec",
  "version": "v1.4.1",
  "commit": "1b06fddd59d8e592d5b5794a1970b22867e85b1f",
  "repository": "https://github.com/Fission-AI/OpenSpec",
  "license": "MIT",
  "localModifications": "None; adapters live outside upstream/."
}
```

Superpowers 使用对应名称、版本、commit 和仓库地址。

- [ ] **Step 4: 发布校验逐文件验证清单与许可证**

`scripts/validate-release.mjs` 对两个 vendor 目录执行：读取 VERSION、校验固定值、解析 MANIFEST、拒绝缺失/额外/摘要不一致文件，并验证 `upstream/LICENSE` 存在。

- [ ] **Step 5: 验证并提交**

Run: `npx vitest run packages/core/test/dependency-metadata.test.ts packages/adapters-test/release-validation.test.ts`

Expected: PASS。

```bash
git add vendor scripts/vendor-manifest.mjs scripts/validate-release.mjs THIRD_PARTY_NOTICES.md packages/core/test/dependency-metadata.test.ts packages/adapters-test/release-validation.test.ts
git commit -m "build: 固定 OpenSpec 与 Superpowers 上游快照"
```

### Task 2: 建立 OpenSpec 结构化领域模型与校验器

**Files:**

- Create: `packages/core/src/engines/openspec/model.ts`
- Create: `packages/core/src/engines/openspec/parser.ts`
- Create: `packages/core/src/engines/openspec/validator.ts`
- Create: `packages/core/src/engines/openspec/renderer.ts`
- Test: `packages/core/test/openspec-engine.test.ts`

- [ ] **Step 1: 写 Requirement、Scenario、Delta 的失败测试**

```ts
it("解析并校验 OpenSpec 风格 requirement 与 scenario", () => {
  const document = parseSpec(`# Order Specification
## ADDED Requirements
### Requirement: Cancel pending order
The system SHALL allow an authorized user to cancel a pending order.
#### Scenario: cancellation succeeds
- GIVEN an authorized user and a pending order
- WHEN the user cancels the order
- THEN the order status is cancelled`);
  expect(validateSpec(document)).toEqual([]);
  expect(document.requirements[0]).toMatchObject({
    id: "REQ-001",
    operation: "ADDED",
    scenarios: [{ id: "REQ-001-SC-001" }],
  });
});

it("拒绝没有 SHALL/MUST 或 Scenario 的 requirement", () => {
  const failures = validateSpec({
    title: "Order",
    requirements: [
      {
        id: "REQ-001",
        title: "Cancel",
        statement: "Cancel order",
        operation: "ADDED",
        scenarios: [],
      },
    ],
  });
  expect(failures.map((entry) => entry.code)).toEqual([
    "SPEC_NORMATIVE_KEYWORD_REQUIRED",
    "SPEC_SCENARIO_REQUIRED",
  ]);
});
```

- [ ] **Step 2: 运行测试并确认 RED**

Run: `npx vitest run packages/core/test/openspec-engine.test.ts`

Expected: FAIL，模块不存在。

- [ ] **Step 3: 实现稳定模型**

```ts
export type DeltaOperation = "ADDED" | "MODIFIED" | "REMOVED";
export interface SpecScenario {
  id: string;
  title: string;
  given: string[];
  when: string[];
  then: string[];
}
export interface SpecRequirement {
  id: string;
  title: string;
  statement: string;
  operation: DeltaOperation;
  scenarios: SpecScenario[];
}
export interface SpecDocument {
  title: string;
  requirements: SpecRequirement[];
}
export interface SpecValidationFailure {
  code:
    | "SPEC_NORMATIVE_KEYWORD_REQUIRED"
    | "SPEC_SCENARIO_REQUIRED"
    | "SPEC_DUPLICATE_ID"
    | "SPEC_DELTA_CONFLICT";
  path: string;
  message: string;
}
```

Parser 按标题层级生成稳定顺序 ID；validator 检查规范关键字、至少一个 Scenario、唯一 ID，以及同标题互斥 delta；renderer 输出 OpenSpec 风格 Markdown，并保持解析后再渲染的稳定性。

- [ ] **Step 4: 增加 round-trip 与冲突测试并验证 GREEN**

Run: `npx vitest run packages/core/test/openspec-engine.test.ts`

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add packages/core/src/engines/openspec packages/core/test/openspec-engine.test.ts
git commit -m "feat: 内置 OpenSpec 结构化规格模型"
```

### Task 3: 用结构化 OpenSpec 能力重构 SpecEngine

**Files:**

- Modify: `packages/core/src/engines/spec/spec-engine.ts`
- Modify: `packages/core/src/commands/new.ts`
- Modify: `packages/core/src/index.ts`
- Test: `packages/core/test/spec-engine.test.ts`
- Test: `packages/core/test/new.test.ts`

- [ ] **Step 1: 写多需求、场景和 BLOCKER 的失败测试**

```ts
it("从完整需求生成多个可追踪 requirement 和 scenario", () => {
  const result = new SpecEngine().generate({
    requirement:
      "授权用户可以通过 API 取消待处理订单；重复取消返回冲突错误；每次成功取消写审计日志；需要成功、未授权和冲突自动化测试。",
    codebaseSummary: "OrderController -> OrderService -> OrderRepository",
  });
  expect(result.model.requirements).toHaveLength(3);
  expect(result.spec).toContain("### Requirement:");
  expect(result.spec).toContain("#### Scenario:");
  expect(result.spec).toContain("The system SHALL");
  expect(result.delta).toContain("## ADDED Requirements");
});
```

将 `SpecArtifacts` 扩展为：

```ts
export interface SpecArtifacts {
  proposal: string;
  impact: string;
  questions: string;
  answers: string;
  assumptions: string;
  spec: string;
  delta: string;
  model: SpecDocument;
}
```

- [ ] **Step 2: 运行测试并确认 RED**

Run: `npx vitest run packages/core/test/spec-engine.test.ts packages/core/test/new.test.ts`

Expected: FAIL，`delta/model` 缺失且当前仅生成 `REQ-001`。

- [ ] **Step 3: 实现需求分句、意图分类和场景生成**

将中文/英文分号、句号和换行切分为行为条目；按成功行为、拒绝/冲突行为、审计行为生成独立 Requirement。只有输入明确包含 actor、action、precondition/result、failure 和 test intent，或 answers 补齐这些信息时才解除 BLOCKER。禁止用长度阈值判断完整性。

`runNew` 除现有制品外写入 `spec.delta.md` 和 `spec.model.json`，均通过 `ArtifactWriter` 生成 metadata。

- [ ] **Step 4: 验证 GREEN 与 candidate 保护**

Run: `npx vitest run packages/core/test/spec-engine.test.ts packages/core/test/new.test.ts`

Expected: PASS，并证明人工修改后的 `spec.md` 不被静默覆盖。

- [ ] **Step 5: 提交**

```bash
git add packages/core/src/engines/spec packages/core/src/commands/new.ts packages/core/src/index.ts packages/core/test/spec-engine.test.ts packages/core/test/new.test.ts
git commit -m "feat: 以 OpenSpec 语义生成结构化规格"
```

### Task 4: 建立 Superpowers 工作流协议和原子计划器

**Files:**

- Create: `packages/core/src/engines/superpowers/protocol.ts`
- Create: `packages/core/src/engines/superpowers/planner.ts`
- Create: `packages/core/src/engines/superpowers/project-commands.ts`
- Modify: `packages/core/src/engines/tdd/tdd-engine.ts`
- Test: `packages/core/test/superpowers-engine.test.ts`
- Test: `packages/core/test/design-plan.test.ts`

- [ ] **Step 1: 写原子任务、依赖和实际命令识别的失败测试**

```ts
it("每个 requirement 生成测试优先的原子任务链", () => {
  const plan = new TddEngine().generatePlan({
    spec: `# Spec
## ADDED Requirements
### Requirement: Cancel order
The system SHALL cancel pending orders.
#### Scenario: success
- GIVEN a pending order
- WHEN cancellation is requested
- THEN the order is cancelled`,
    design: "OrderController -> OrderService -> OrderRepository",
    impact: "src/order.ts\ntest/order.test.ts",
    codebaseSummary: "package.json\nsrc/order.ts\ntest/order.test.ts",
  });
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
  expect(plan.tasks[1].dependsOn).toEqual([plan.tasks[0].id]);
  expect(plan.tasks[0].allowedFiles).not.toContain("src/**");
});

it("根据项目文件选择验证命令", () => {
  expect(detectProjectCommands(["pom.xml"])).toEqual([
    "mvn test",
    "mvn verify",
  ]);
  expect(detectProjectCommands(["package.json"])).toContain("npm test");
});
```

- [ ] **Step 2: 运行测试并确认 RED**

Run: `npx vitest run packages/core/test/superpowers-engine.test.ts packages/core/test/design-plan.test.ts`

Expected: FAIL，当前永远只生成一个任务且固定使用 `npm test`。

- [ ] **Step 3: 扩展任务协议**

```ts
export type TddPhase = "RED" | "GREEN" | "REFACTOR" | "VERIFY";
export interface TaskDefinition {
  id: string;
  title: string;
  phase: TddPhase;
  status: "PENDING" | "BUILDING" | "DONE" | "FAILED" | "SKIPPED";
  requirements: string[];
  scenarios: string[];
  dependsOn: string[];
  allowedFiles: string[];
  expectedNewFiles: string[];
  forbiddenFiles: string[];
  verification: string[];
  doneCriteria: string[];
}
```

Planner 按每个 Requirement 生成四阶段链，跨 Requirement 仅在文件范围重叠时增加依赖。文件范围由 impact/codebase summary 中的候选路径构造；无法推导时生成 BLOCKER，而不是退化到仓库级通配符。`project-commands.ts` 仅返回 shell-policy 允许的 Maven/npm 命令。

- [ ] **Step 4: 更新 Markdown、JSON、Context Pack 并验证 GREEN**

tasks.md 每个任务必须包含 `Phase`、Requirements、Scenarios、Depends On、Allowed Files、Verification 和 Done Criteria。Context Pack 保留 30 KB 限制及三类 hash 元数据。

Run: `npx vitest run packages/core/test/superpowers-engine.test.ts packages/core/test/design-plan.test.ts`

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add packages/core/src/engines packages/core/test/superpowers-engine.test.ts packages/core/test/design-plan.test.ts
git commit -m "feat: 内置 Superpowers 原子 TDD 计划协议"
```

### Task 5: 强制 RED→GREEN→REFACTOR→VERIFY 执行证据

**Files:**

- Modify: `packages/core/src/build/task-executor.ts`
- Modify: `packages/core/src/commands/build.ts`
- Modify: `packages/core/src/quality/quality-gates.ts`
- Test: `packages/core/test/build.test.ts`
- Test: `packages/core/test/quality-commands.test.ts`

- [ ] **Step 1: 写缺少 RED 证据时失败的测试**

```ts
it("没有预期失败的 RED 证据时拒绝完成任务链", async () => {
  const executor = new RecordingExecutor({
    modifiedFiles: ["src/order.ts"],
    tddEvidence: [
      { phase: "GREEN", command: "npm test", passed: true, output: "1 passed" },
    ],
    verification: [{ command: "npm test", passed: true, output: "1 passed" }],
  });
  const result = await core.execute({ command: "build", cwd: root });
  expect(result).toMatchObject({
    ok: false,
    error: { code: "E_TDD_EVIDENCE_REQUIRED" },
  });
});
```

- [ ] **Step 2: 运行测试并确认 RED**

Run: `npx vitest run packages/core/test/build.test.ts packages/core/test/quality-commands.test.ts`

Expected: FAIL，错误码和 `tddEvidence` 尚不存在。

- [ ] **Step 3: 增加证据契约和错误码**

```ts
export interface TddEvidence {
  phase: "RED" | "GREEN" | "REFACTOR" | "VERIFY";
  command: string;
  passed: boolean;
  expectedFailure?: boolean;
  output: string;
}
export interface TaskExecutionResult {
  modifiedFiles: string[];
  tddEvidence: TddEvidence[];
  verification: VerificationEvidence[];
}
```

在 `contracts.ts` 增加 `E_TDD_EVIDENCE_REQUIRED: 7`。build 按 Requirement 任务链验证：RED 必须 `passed=false && expectedFailure=true`，GREEN/VERIFY 必须通过，REFACTOR 必须出现在两者之间；命令均通过 shell-policy。

- [ ] **Step 4: verifyGate 复核持久化证据并验证 GREEN**

Run: `npx vitest run packages/core/test/build.test.ts packages/core/test/quality-commands.test.ts`

Expected: PASS；缺少、乱序或伪造通过状态均失败。

- [ ] **Step 5: 提交**

```bash
git add packages/core/src/build/task-executor.ts packages/core/src/commands/build.ts packages/core/src/quality/quality-gates.ts packages/core/src/contracts.ts packages/core/test/build.test.ts packages/core/test/quality-commands.test.ts
git commit -m "feat: 强制记录并校验 TDD 阶段证据"
```

### Task 6: 完成 Scenario 级追踪与归档闸门

**Files:**

- Modify: `packages/core/src/quality/quality-gates.ts`
- Modify: `packages/core/src/commands/verify.ts`
- Modify: `packages/core/src/commands/archive.ts`
- Test: `packages/core/test/quality-commands.test.ts`

- [ ] **Step 1: 写 Scenario 无测试映射时失败的测试**

```ts
it("scenario 没有关联任务与测试证据时阻止 verify 和 archive", () => {
  const result = verifyGate(spec, tasksWithoutScenario, taskResults, statuses);
  expect(result.failures).toContain("REQ-001-SC-002 未关联到任务和测试证据");
});
```

- [ ] **Step 2: 运行测试并确认 RED**

Run: `npx vitest run packages/core/test/quality-commands.test.ts`

Expected: FAIL，当前只检查 Requirement 是否关联任务。

- [ ] **Step 3: 生成完整追踪模型**

verifyGate 解析 `spec.model.json`，逐项验证 Requirement → Scenario → Task → TddEvidence/Verification。archive 的 `traceability.md` 对每个 Scenario 输出任务、修改文件、RED 命令、GREEN 命令和最终 VERIFY 命令；任一为空时返回 `E_VERIFY_REQUIRED`。

- [ ] **Step 4: 运行质量与归档测试**

Run: `npx vitest run packages/core/test/quality-commands.test.ts`

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add packages/core/src/quality/quality-gates.ts packages/core/src/commands/verify.ts packages/core/src/commands/archive.ts packages/core/test/quality-commands.test.ts
git commit -m "feat: 增加场景级需求追踪与归档闸门"
```

### Task 7: 补齐端到端验收和跨平台 CI

**Files:**

- Create: `.github/workflows/ci.yml`
- Modify: `test/e2e/acceptance.test.ts`
- Modify: `test/e2e/workflow.test.ts`
- Modify: `packages/adapters-test/adapter-contract.test.ts`
- Test: `test/e2e/acceptance.test.ts`
- Test: `test/e2e/workflow.test.ts`

- [ ] **Step 1: 写两个平台相同结构化制品的失败测试**

```ts
it("两个 Adapter 的完整流程均生成可验证追踪", async () => {
  for (const [name, createAdapter, autoCommand] of [
    ["claude", (value: Core) => new ClaudeCodeAdapter(value), "/sdd.auto"],
    ["codex", (value: Core) => new CodexAdapter(value), "sdd auto"],
  ] as const) {
    const root = await fixture(name);
    const adapter = createAdapter(core());
    await adapter.execute(name === "claude" ? "/sdd.init" : "sdd init", root);
    const result = await adapter.execute(
      `${autoCommand} "${requirement}" --change add-cancel`,
      root,
    );
    expect(result.state).toBe("ARCHIVED");
    const change = join(root, ".sdd/changes/add-cancel");
    expect(await readFile(join(change, "spec.delta.md"), "utf8")).toContain(
      "## ADDED Requirements",
    );
    expect(await readFile(join(change, "tasks.md"), "utf8")).toContain(
      "Phase: RED",
    );
    expect(await readFile(join(change, "traceability.md"), "utf8")).toContain(
      "REQ-001-SC-001",
    );
  }
});
```

- [ ] **Step 2: 运行 E2E 并确认 RED**

Run: `npx vitest run test/e2e/acceptance.test.ts test/e2e/workflow.test.ts packages/adapters-test/adapter-contract.test.ts`

Expected: FAIL，结构化 delta/TDD/Scenario 追踪尚未贯通。

- [ ] **Step 3: 创建 macOS/Windows × Node 20/22 CI**

```yaml
name: CI
on: [push, pull_request]
jobs:
  verify:
    strategy:
      fail-fast: false
      matrix:
        os: [macos-latest, windows-latest]
        node: [20, 22]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: ${{ matrix.node }}
          cache: npm
      - run: npm ci
      - run: npm run format:check
      - run: npm run lint
      - run: npm run typecheck
      - run: npm test
      - run: npm run validate:schemas
      - run: npm run validate:release
```

Adapter 契约测试在每个操作系统任务中同时覆盖 Claude Code 和 Codex，因此四种 OS × Agent 组合均有自动证据。

- [ ] **Step 4: 验证 E2E 与发布校验**

Run: `npm test && npm run validate:schemas && npm run validate:release`

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add .github/workflows/ci.yml test/e2e packages/adapters-test/adapter-contract.test.ts
git commit -m "test: 补齐跨平台结构化工作流验收"
```

### Task 8: 更新中文文档与逐条完成审计

**Files:**

- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/requirements-traceability.md`
- Modify: `docs/需求文档.md`
- Modify: `docs/command-contract.md`

- [ ] **Step 1: 将追踪矩阵改为可核验条目**

每行必须包含需求章节、实现符号、测试名称、验证命令和状态；禁止仅以“文件存在”作为完成证据。至少单列 3.3/4.2 OpenSpec、3.4/4.3 Superpowers、23.2–23.8 命令职责、24.5 平台矩阵和 26.1–26.3 验收条目。

- [ ] **Step 2: 更新用户与架构文档**

README 说明结构化 spec/delta、TDD 证据和失败恢复；architecture 说明 vendor 基线、适配层与 `.sdd/` 边界；需求文档状态仅在全部证据通过后从 Draft 更新为 Implemented，并记录实现日期。

- [ ] **Step 3: 执行占位符和格式检查**

Run: `rg -n "T[B]D|T[O]DO|place[Hh]older|待实现" README.md docs packages/core/src`

Expected: 无未解释的实现占位符。

- [ ] **Step 4: 执行最终验证**

```bash
npm run format:check
npm run lint
npm run typecheck
npm test
npm run test:coverage
npm run validate:schemas
npm run validate:release
git diff --check
```

Expected: 所有命令退出码为 0；全量测试无失败；覆盖率报告生成；vendor、Schema 和发布布局校验通过。

- [ ] **Step 5: 逐条复核并提交**

逐项对照 `docs/需求文档.md` 的 MVP0–MVP2、命令职责和最终验收标准；任何证据缺失都返回对应 Task 修复，不以现有测试绿色替代需求证据。

```bash
git add README.md docs
git commit -m "docs: 完成需求能力与验证证据追踪"
```
