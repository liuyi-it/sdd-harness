# Phase II-A Structured Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Schema、项目规范、Loop 和 TaskExecutor 契约统一升级到 1.2.0，同时兼容一期制品。

**Architecture:** Core 继续作为唯一状态写入者；新增独立的 schema migration、project conventions、loop store 和 execution normalization 模块。所有新状态通过 StateStore 与 ArtifactWriter 原子落盘，Adapter 只传递参数和宿主能力。

**Tech Stack:** TypeScript、Node.js 20/22、Zod、JSON Schema 2020-12、Vitest、YAML

---

### Task 1: Schema 1.2.0 与真实模型对齐

**Files:**

- Modify: `schemas/task.schema.json`
- Create: `schemas/task-execution-result.schema.json`
- Create: `schemas/loop.schema.json`
- Create: `schemas/loop-run.schema.json`
- Modify: `packages/core/src/install/canonical-schemas.ts`
- Test: `packages/core/test/schema-golden.test.ts`

- [ ] **Step 1: 写失败测试**，构造真实 `TddEngine` 任务并断言包含 `phase/scenarios` 的任务通过新 Schema；断言缺少二者的 v1.2 任务失败。
- [ ] **Step 2: 验证红灯**：运行 `npx vitest run packages/core/test/schema-golden.test.ts`，预期因 Schema 缺失或不匹配而失败。
- [ ] **Step 3: 实现 Schema**：任务 ID 使用 `^TASK-[0-9]{3}(?:-(?:RED|GREEN|REFACTOR|VERIFY))?$`；新增制品均要求 `schemaVersion: "1.2.0"`，结构化命令统一为：

```ts
interface StructuredCommand {
  command: string;
  args: string[];
}
```

- [ ] **Step 4: 注册 canonical schemas**，确保 init 安装全部四份新 Schema。
- [ ] **Step 5: 验证绿灯**：运行目标测试及 `npm run validate:schemas`，预期全部通过。
- [ ] **Step 6: 提交**：`git commit -m "feat: 升级二期基础 Schema 到 1.2.0"`。

### Task 2: 1.0.0 到 1.2.0 迁移

**Files:**

- Create: `packages/core/src/state/schema-migration.ts`
- Modify: `packages/core/src/state/state-store.ts`
- Modify: `packages/core/src/commands/init.ts`
- Modify: `schemas/state.schema.json`
- Modify: `schemas/config.schema.json`
- Test: `packages/core/test/state.test.ts`
- Test: `packages/core/test/init-status.test.ts`

- [ ] **Step 1: 写迁移失败测试**：从 1.0.0 fixture 读取 state，断言备份、`migration-report.md`、migration log、`activeLoop: null` 和 1.2.0 config 均存在。
- [ ] **Step 2: 运行目标测试确认失败**。
- [ ] **Step 3: 定义迁移结果**：

```ts
interface MigrationResult {
  from: "1.0.0";
  to: "1.2.0";
  state: WorkflowState;
  backupPaths: string[];
}
```

- [ ] **Step 4: 实现显式迁移**：只接受 1.0.0 和 1.2.0；未知版本返回 `E_STATE_CORRUPTED`，迁移前复制 state/config，迁移后原子写入报告。
- [ ] **Step 5: 更新 Zod 与 JSON Schema**，使 state/config 都只写 1.2.0。
- [ ] **Step 6: 运行 `npx vitest run packages/core/test/state.test.ts packages/core/test/init-status.test.ts`**，预期通过。
- [ ] **Step 7: 提交**：`git commit -m "feat: 添加 1.2.0 状态迁移"`。

### Task 3: 项目规范画像与空项目确认

**Files:**

- Create: `packages/core/src/project-conventions/model.ts`
- Create: `packages/core/src/project-conventions/scanner.ts`
- Create: `packages/core/src/project-conventions/store.ts`
- Modify: `packages/core/src/commands/init.ts`
- Test: `packages/core/test/project-conventions.test.ts`
- Test: `packages/core/test/init-status.test.ts`

- [ ] **Step 1: 写失败测试**：空目录 init 未给策略时返回 `CLARIFYING`；传 `structurePolicy: "free-design"` 后生成规范；已有项目生成带证据路径的 discovered 规范。
- [ ] **Step 2: 运行目标测试确认失败**。
- [ ] **Step 3: 定义模型**：

```ts
interface ProjectConventionProfile {
  schemaVersion: "1.2.0";
  projectType: "empty" | "existing";
  strategy: "free-design" | "user-defined" | "discovered";
  directories: {
    source: string[];
    test: string[];
    assets: string[];
    config: string[];
  };
  conventions: Array<{ kind: string; value: string; evidence: string[] }>;
  unknowns: string[];
  ruleFiles: Array<{ path: string; scope: string; sha256: string }>;
  generatedAt: string;
  indexHash: string;
}
```

- [ ] **Step 4: 实现空项目判定**：忽略 `.git/.sdd` 和纯说明文件；没有源码、构建配置或模块目录才视为空项目。
- [ ] **Step 5: 实现扫描与双格式持久化**：只输出有证据的推断，未知项进入 `unknowns`；重复 init 不覆盖人工规范。
- [ ] **Step 6: 运行目标测试，预期通过**。
- [ ] **Step 7: 提交**：`git commit -m "feat: 初始化项目规范画像"`。

### Task 4: 逐任务规则解析与新鲜度

**Files:**

- Create: `packages/core/src/project-conventions/rule-resolver.ts`
- Modify: `packages/core/src/commands/plan.ts`
- Modify: `packages/core/src/commands/build.ts`
- Modify: `packages/core/src/build/task-executor.ts`
- Test: `packages/core/test/project-rules.test.ts`
- Test: `packages/core/test/design-plan.test.ts`
- Test: `packages/core/test/build.test.ts`

- [ ] **Step 1: 写失败测试**：覆盖根目录/嵌套规则、Codex/Claude 优先级、冲突 CLARIFYING、plan 后规则变化触发 Context Pack 重建。
- [ ] **Step 2: 运行目标测试确认失败**。
- [ ] **Step 3: 定义规则快照**：

```ts
interface ProjectRuleSnapshot {
  host: "codex" | "claude-code";
  sources: Array<{
    path: string;
    scope: string;
    sha256: string;
    priority: number;
  }>;
  acknowledgement: "MUST_FOLLOW_PROJECT_RULES";
}
```

- [ ] **Step 4: 实现解析器**：按任务文件范围选择嵌套文件；当前宿主文件优先、另一文件补充；不可判定冲突返回结构化冲突列表。
- [ ] **Step 5: 注入 Context Pack 和请求**，并将可机器判断的文件/命令约束合并到 constraints。
- [ ] **Step 6: build 前比较规则哈希和规范版本**，过期时重建 pack；结果记录实际哈希。
- [ ] **Step 7: 运行三个目标测试，预期通过**。
- [ ] **Step 8: 提交**：`git commit -m "feat: 为每个构建任务注入项目规则"`。

### Task 5: Loop Specification 与运行存储

**Files:**

- Create: `packages/core/src/loop/model.ts`
- Create: `packages/core/src/loop/loop-store.ts`
- Create: `packages/core/src/loop/loop-spec.ts`
- Modify: `packages/core/src/commands/init.ts`
- Modify: `packages/core/src/state/state-store.ts`
- Test: `packages/core/test/loop.test.ts`

- [ ] **Step 1: 写失败测试**：init 生成 loop spec；人工修改时生成 candidate；activeLoop 存在而 run 缺失时恢复并标记 recovered。
- [ ] **Step 2: 运行测试确认失败**。
- [ ] **Step 3: 定义 `ActiveLoop`、`LoopRun` 和 `LoopStep`**，全部固定 1.2.0，并实现 schema parse。
- [ ] **Step 4: 实现 loop spec 幂等安装和 LoopStore 原子写入**。
- [ ] **Step 5: 扩展 WorkflowState.activeLoop**，保证 state 为当前事实源、run 为审计历史。
- [ ] **Step 6: 运行 `npx vitest run packages/core/test/loop.test.ts packages/core/test/state.test.ts`**，预期通过。
- [ ] **Step 7: 提交**：`git commit -m "feat: 持久化 SDD Loop 运行记录"`。

### Task 6: Auto resume/restart 编排

**Files:**

- Create: `packages/core/src/commands/auto.ts`
- Modify: `packages/core/src/core.ts`
- Modify: `packages/core/src/contracts.ts`
- Test: `packages/core/test/auto.test.ts`

- [ ] **Step 1: 写失败测试**：默认继续 activeLoop；`resume=<run-id>` 恢复指定运行；`restart=true` 标记旧运行 ABORTED；二者共存返回参数错误。
- [ ] **Step 2: 运行 auto 测试确认失败**。
- [ ] **Step 3: 从 Core 拆出 AutoRunner**，每一步记录 stateBefore/stateAfter、startedAt/endedAt/status，使用 loop spec 的 maxSteps 和 stoppingRules。
- [ ] **Step 4: 实现失败、暂停、澄清、hard stop 与 ARCHIVED 收敛**。
- [ ] **Step 5: 运行 `npx vitest run packages/core/test/auto.test.ts packages/core/test/loop.test.ts`**，预期通过。
- [ ] **Step 6: 提交**：`git commit -m "feat: 将 auto 升级为可恢复 Loop"`。

### Task 7: TaskExecutor v2 与 v1 normalize

**Files:**

- Modify: `packages/core/src/build/task-executor.ts`
- Create: `packages/core/src/build/task-result-normalizer.ts`
- Modify: `packages/core/src/security/shell-policy.ts`
- Modify: `packages/core/src/commands/build.ts`
- Test: `packages/core/test/task-executor-v2.test.ts`
- Test: `packages/core/test/build.test.ts`

- [ ] **Step 1: 写失败测试**：覆盖 v1 补字段、v2 原样保留、结构化命令、危险字符串阻断、subagent 降级记录和 Git delta 裁决。
- [ ] **Step 2: 运行目标测试确认失败**。
- [ ] **Step 3: 实现 v2 request/result 和 `AllowedCommand`**；命令证据改用 command/args/outputSummary。
- [ ] **Step 4: 实现 normalize**：安全拆分仅允许普通 argv；出现 `|`, `>`, `<`, `$(`, `` ` `` 返回 `E_SECURITY_BLOCKED`。
- [ ] **Step 5: build 以 Git delta 覆盖声明文件列表并写运行级结果**。
- [ ] **Step 6: 运行目标测试和 `npm test`，预期全部通过**。
- [ ] **Step 7: 提交**：`git commit -m "feat: 升级 TaskExecutor v2 契约"`。

### Task 8: II-A 文档、全门禁与推送

**Files:**

- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/schemas.md`
- Modify: `docs/state-machine.md`
- Modify: `docs/requirements-traceability.md`

- [ ] 更新中文文档，明确 Schema 1.2、空项目交互、规则契约、Loop 与 v1/v2 兼容。
- [ ] 运行 `npm run format:check && npm run lint && npm run typecheck && npm test && npm run validate:schemas && npm run validate:release`，预期全部退出 0。
- [ ] 运行 `git diff --check`，预期无输出。
- [ ] 提交：`git commit -m "docs: 完成二期 A 阶段说明"`。
- [ ] 推送当前分支并确认远端提交存在后，才开始 II-B。
