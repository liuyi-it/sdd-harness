# Auto Loop Engine 正式化 — 实施设计文档

> 基于 `docs/四期需求文档.md`，日期：2026-07-09

## 1. 目标

保留 `sdd auto` 作为唯一公开 Loop 入口，将其内部升级为可恢复、可观测、可决策的 SDD Loop Engine。不新增 `sdd loop` 公开命令。

## 2. 架构变更

### 2.1 当前架构

```
Core.runAuto() (~90行内联)
  ├── prepareAutoLoop()     → LoopStore
  ├── autoCommand()         → 纯函数，phase→command 映射
  ├── Core.execute()        → 复用，执行单阶段命令
  ├── recordAutoStep()      → LoopStore.writeRun()
  └── finalizeAutoLoop()    → LoopStore.writeRun() + StateStore.update()
```

### 2.2 目标架构（M3 后）

```
Core.runAuto() ──薄封装 (~5行)──→ LoopEngine.run(request)
                                      │
                                      ├── LoopStore       → 读写 run/spec/events
                                      ├── DecisionEngine  → 纯函数，规则决策
                                      ├── LoopEventStore  → JSONL 写入
                                      └── Core.execute()  → 复用现有单命令
```

### 2.3 新增/修改文件清单

| 文件                                          | 操作                                                                               | Milestone |
| --------------------------------------------- | ---------------------------------------------------------------------------------- | --------- |
| `packages/core/src/contracts.ts`              | 修改：PHASES 新增 `BUILD_WAITING_AGENT`                                            | M1        |
| `packages/core/src/commands/build.ts`         | 修改：buildNextTask/buildCompleteTask 状态写入                                     | M1, M2    |
| `packages/core/src/state/state-store.ts`      | 确认 normalizeTransientState 不漏处理 BUILD_WAITING_AGENT                          | M1        |
| `packages/core/src/state/schema-migration.ts` | 修改：升级 CURRENT_SCHEMA_VERSION 到 1.3.0，增加 BUILDING→BUILD_WAITING_AGENT 迁移 | M1        |
| `packages/core/src/commands/design.ts`        | 修改：unchanged 早退时写回 DESIGN_READY                                            | M1        |
| `packages/core/src/commands/plan.ts`          | 修改：unchanged 早退时写回 PLAN_READY                                              | M1        |
| `packages/core/src/commands/status.ts`        | 修改：PLAN_READY next 改为 `sdd build next`                                        | M1        |
| `packages/core/src/loop/model.ts`             | 修改：LoopSpec/ActiveLoop/LoopRun/LoopStep 升级到 1.3.0                            | M3        |
| `packages/core/src/loop/loop-spec.ts`         | 修改：createDefaultLoopSpec 升级到 1.3.0                                           | M3        |
| `packages/core/src/loop/loop-store.ts`        | 修改：新增 events 读写方法                                                         | M3        |
| `packages/core/src/loop/loop-engine.ts`       | **新增**：LoopEngine 主类                                                          | M3        |
| `packages/core/src/loop/loop-decision.ts`     | **新增**：DecisionEngine 纯函数                                                    | M3        |
| `packages/core/src/loop/loop-events.ts`       | **新增**：LoopEventStore JSONL                                                     | M3        |
| `packages/core/src/commands/auto.ts`          | 修改：功能迁入 LoopEngine，保留兼容层                                              | M3        |
| `packages/core/src/core.ts`                   | 修改：runAuto() 改为薄封装                                                         | M3        |
| `packages/cli/src/cli.ts`                     | 修改：新增 flags + help 更新                                                       | M4        |
| `packages/cli/src/commands/auto.ts`           | 修改：支持新 flags 透传                                                            | M4        |
| `packages/cli/src/commands/status.ts`         | 修改：支持 --loop                                                                  | M4        |
| `packages/cli/src/json-output.ts`             | 修改：outputText 显示 actionRequired                                               | M4        |

---

## 3. Milestone 1：Handoff 状态稳定化

### 3.1 新增 `BUILD_WAITING_AGENT` Phase

**变更位置**：`packages/core/src/contracts.ts`

在 `PHASES` 数组中 `"BUILDING"` 之后新增 `"BUILD_WAITING_AGENT"`。

`TRANSIENT_PHASE_RECOVERY`（state-store.ts）**不添加** BUILD_WAITING_AGENT —— 它是 stable phase，不需要 transient recovery。

### 3.2 `build next` 状态写入

**变更位置**：`packages/core/src/commands/build.ts` → `buildNextTask()`

当前行为（第 745-753 行）：

- `currentPhase = "BUILDING"`
- `inProgressPhase = "BUILDING"`
- `suggestedCommand = "sdd build next"`
- 返回 `state: "BUILDING"`

目标行为：

- `currentPhase = "BUILD_WAITING_AGENT"`
- `inProgressPhase = null`（不再标记为进行中）
- `suggestedCommand = "sdd build complete"`
- `activeLoop.status = "WAITING_AGENT"`
- `activeLoop.waiting = { reason: "AGENT_TASK_EXECUTION", taskId, resultFile, since }`
- 返回 `state: "BUILD_WAITING_AGENT"`

### 3.3 重复 `build next` 不重复分配任务

**变更位置**：同上

在 `buildNextTask()` 开头增加判断：如果 `state.currentPhase === "BUILD_WAITING_AGENT"` 且 `activeLoop.waiting.taskId` 存在，返回同一个 `actionRequired`（通过读取已有 context pack 重建），不创建新任务。

### 3.4 `build complete` 状态写入

**变更位置**：`packages/core/src/commands/build.ts` → `buildCompleteTask()`

当前行为（第 986-997 行）：

- 还有任务未完成 → `currentPhase = "BUILDING"`，`suggestedCommand = "sdd build next"`
- 全部完成 → `currentPhase = "BUILD_READY"`，`suggestedCommand = "sdd verify"`

目标行为：

- 还有任务 → `currentPhase = "PLAN_READY"`，`suggestedCommand = "sdd build next"`
- 全部完成 → `currentPhase = "BUILD_READY"`，`suggestedCommand = "sdd verify"`
- 清除 `activeLoop.waiting`

### 3.5 `normalizeTransientState()` 不变

`BUILD_WAITING_AGENT` 不在 `TRANSIENT_PHASE_RECOVERY` 中，`normalizeTransientState` 会通过 `if (recovery === undefined) return state;` 自然跳过，**不需要修改代码**。

`BUILDING` 仍在 `TRANSIENT_PHASE_RECOVERY` 中（传统完整 build 流程），其逻辑不受影响——因为 `build next/complete` 流不再经过此路径。

### 3.6 Schema 迁移 1.2.0 → 1.3.0

**变更位置**：`packages/core/src/state/schema-migration.ts`

```ts
export const CURRENT_SCHEMA_VERSION = "1.3.0";
```

新增迁移逻辑（在 `StateStore.read()` 第 170 行附近，与现有 1.0.0→1.2.0 迁移 Pattern 一致，增加 1.2.0→1.3.0 的内联检测）：

- 检测 `currentPhase === "BUILDING"` 且 `suggestedCommand` 包含 `"build next"` 或 `"build complete"`
- 迁移为 `BUILD_WAITING_AGENT`
- 设置 `activeLoop.status = "WAITING_AGENT"`，`activeLoop.waiting.taskId = <推断的taskId>`
- 如果无法确定唯一 taskId，标记为 `FAILED`

### 3.7 `design/plan` unchanged 状态收敛

**变更位置**：`packages/core/src/commands/design.ts`

当前（第 103-112 行）：unchanged 时直接 `return { state: "DESIGN_READY" }`，不写 store。

改为在 return 前调用 `store.update()`：

```ts
if (unchanged) {
  await store.update((current) => ({
    ...current,
    currentPhase: "DESIGN_READY",
    inProgressPhase: null,
    suggestedCommand: "sdd plan",
    artifacts: { ...current.artifacts, design: "READY" },
  }));
  return { ok: true, state: "DESIGN_READY", ... };
}
```

**变更位置**：`packages/core/src/commands/plan.ts`（第 124-133 行）

同理，unchanged 时写回 `PLAN_READY` + 制品状态。

### 3.8 `PLAN_READY` 的 status next 修改

**变更位置**：`packages/core/src/commands/status.ts`（第 60 行）、`packages/core/src/state/state-store.ts`（第 431 行）

```ts
// 从:
PLAN_READY: "sdd build";
// 改为:
PLAN_READY: "sdd build next";
```

**涉及测试用例（M1）**：AC-006, AC-007, AC-008, AC-015, AC-016, AC-017, AC-018, AC-020, AC-033

---

## 4. Milestone 2：基础安全修复

### 4.1 `build complete` taskId 一致性校验

**变更位置**：`packages/core/src/commands/build.ts` → `buildCompleteTask()`

在 `resultJson` 读取后校验 `resultJson.taskId === rawArgs.taskId`，不一致返回 `E_STATE_CORRUPTED`。

### 4.2 `build complete` 当前等待任务校验

如果 `state.currentPhase === "BUILD_WAITING_AGENT"`，校验 `activeLoop.waiting.taskId === rawArgs.taskId`，不一致拒绝。

### 4.3 文件范围校验改用 `validateTaskFiles()`

**变更位置**：`packages/core/src/commands/build.ts` → `buildCompleteTask()`

删除当前 out-of-scope 手动判断逻辑（第 867-886 行），统一调用 `validateTaskFiles(modifiedFiles, task)`。

`validateTaskFiles()` 的正确逻辑（来自 `security/task-scope.ts`）：

1. 命中 `forbiddenFiles` → `E_SECURITY_BLOCKED`
2. 不在 `allowedFiles + expectedNewFiles` → `E_SECURITY_BLOCKED`

### 4.4 TDD evidence 校验加强

在 `build complete` 的 evidence 校验中增加：

- 每项的 `command` 必须通过 `isCommandAllowed` 校验
- 与 `task.phase` 不匹配时失败

### 4.5 `AgentActionRequired.codebase` 从 state 读取

**变更位置**：`packages/core/src/commands/build.ts` → `buildNextTask()`

当前硬编码：

```ts
codebase: { provider: "fallback-file-scan", degraded: true }
```

改为：

```ts
codebase: {
  provider: state.codebaseProvider === "codebase-memory-mcp"
    ? "codebase-memory-mcp"
    : "fallback-file-scan",
  degraded: state.degraded,
}
```

**涉及测试用例（M2）**：AC-010, AC-011, AC-012, AC-013, AC-014, AC-021

---

## 5. Milestone 3：Auto LoopEngine 抽象

### 5.1 新增文件与职责

```
packages/core/src/loop/
  loop-engine.ts    ← LoopEngine 主类：run/resume/restart/stop/events/status
  loop-decision.ts  ← DecisionEngine：纯函数，CommandResult → LoopDecision
  loop-events.ts    ← LoopEventStore：JSONL 读写
```

### 5.2 LoopEngine 设计

```ts
export class LoopEngine {
  constructor(
    private readonly root: string,
    private readonly store: StateStore,
    private readonly loops: LoopStore,
    private readonly events: LoopEventStore,
    private readonly execute: (req: CommandRequest) => Promise<CommandResult>,
  ) {}

  async run(request: CommandRequest): Promise<CommandResult> {
    // 分发
    if (args.resume) return this.resume(args.resume);
    if (args.restart) return this.restart();
    if (args.stop) return this.stop();
    if (args.events) return this.getEvents(args.tail);
    if (args.loopStatus) return this.getLoopStatus();
    return this.runAuto(request);
  }

  private async runAuto(request): Promise<CommandResult> {
    // 从现有 Core.runAuto() 迁移
    // 增加：DecisionEngine.decide() 调用（每一步后）
    // 增加：LoopEventStore 写入（run started / command started / command finished / decision made）
  }

  async resume(runId?: string): Promise<CommandResult> {
    /* 新实现 */
  }
  async restart(): Promise<CommandResult> {
    /* 迁移自 prepareAutoLoop */
  }
  async stop(): Promise<CommandResult> {
    /* 新实现 */
  }
  async getEvents(opts?: { tail?: number }): Promise<CommandResult> {
    /* 新实现 */
  }
  async getLoopStatus(): Promise<CommandResult> {
    /* 新实现 */
  }
}
```

### 5.3 DecisionEngine 设计

```ts
// 纯函数，可独立单元测试
export function decide(input: {
  result: CommandResult;
  currentPhase: Phase;
}): LoopDecision {
  if (!input.result.ok) {
    if (input.result.error?.code === "E_SECURITY_BLOCKED") return "FAIL";
    if (input.result.error?.code === "E_STATE_CORRUPTED") return "FAIL";
    return "FAIL";
  }
  if (input.result.state === "CLARIFYING") return "PAUSE_FOR_CLARIFICATION";
  if (input.result.actionRequired?.type === "AGENT_TASK_EXECUTION")
    return "PAUSE_FOR_AGENT";
  if (input.result.state === "BUILD_WAITING_AGENT") return "PAUSE_FOR_AGENT";
  if (input.result.state === "BUILD_READY") return "CONTINUE";
  if (input.result.state === "VERIFY_READY") return "CONTINUE";
  if (input.result.state === "REVIEW_READY") return "CONTINUE";
  if (input.result.state === "ARCHIVED") return "DONE";
  if (input.result.error?.code === "E_VERIFY_FAILED") return "PAUSE_FOR_HUMAN";
  if (input.result.error?.code === "E_REVIEW_FAILED") return "PAUSE_FOR_HUMAN";
  return "CONTINUE";
}
```

### 5.4 Core.runAuto() 薄封装

```ts
// core.ts — 从 ~90 行缩减为 ~5 行
private async runAuto(request: CommandRequest): Promise<CommandResult> {
  return new LoopEngine(
    request.cwd,
    new StateStore(request.cwd),
    new LoopStore(request.cwd),
    new LoopEventStore(request.cwd),
    (req) => this.execute(req),
  ).run(request);
}
```

### 5.5 数据模型升级（1.3.0）

**变更位置**：`packages/core/src/loop/model.ts`

`LoopSpec` 新增字段：`maxRetriesPerStep`, `maxRepeatedFailures`, `decisionPolicy`, `stoppingRules` 扩展

`ActiveLoop` 新增字段：`WAITING_AGENT` 状态，`waiting` 对象

`LoopRun` 新增字段：`changeId`, `currentStep`, `updatedAt`, `lastDecision`, `waiting`

`LoopStep` 新增字段：`kind`, `phaseBefore`, `phaseAfter`, `decision`, `actionRequired`, `error`, `warnings`, `artifacts`

**变更位置**：`packages/core/src/loop/loop-spec.ts`

`createDefaultLoopSpec()` 升级 `schemaVersion` 到 `"1.3.0"`，`maxSteps` 从 8 调整为 12，新增默认值。

**涉及测试用例（M3）**：AC-001, AC-002, AC-003, AC-004, AC-005, AC-022, AC-023, AC-024

---

## 6. Milestone 4：Auto 命令增强

### 6.1 CLI 解析

**变更位置**：`packages/cli/src/cli.ts`

在 `parseArgs` 的 options 中新增 flags：

```ts
resume:     { type: "boolean", default: false },
restart:    { type: "boolean", default: false },
stop:       { type: "boolean", default: false },
events:     { type: "boolean", default: false },
tail:       { type: "string" },
loop:       { type: "boolean", default: false },
loopStatus: { type: "boolean", default: false },
run:        { type: "string" },
```

`case "auto"` 分支将新 flags 映射到 `extraArgs`：

- `--resume` → `extraArgs.resume = values.run ?? true`
- `--restart` → `extraArgs.restart = true`
- `--stop` → `extraArgs.stop = true`
- `--events` → `extraArgs.events = true`，`extraArgs.tail = Number(values.tail)`
- `--loop-status`(或 `--status`) → `extraArgs.loopStatus = true`

### 6.2 status --loop

**变更位置**：`packages/cli/src/commands/status.ts` + `packages/core/src/commands/status.ts`

当 `args.loop === true` 时，`runStatus()` 返回 data 中包含 activeLoop 摘要。

### 6.3 文本输出显示 actionRequired

**变更位置**：`packages/cli/src/json-output.ts` → `outputText()`

当 `result.actionRequired` 存在时，打印：

```
Action Required: <type>
Task: <taskId>
Context Pack: <contextPack>
Result File: <resultFile>

Allowed Files:
  - ...

Expected New Files:
  - ...

Forbidden Files:
  - ...

Verification:
  - <command> <args>
```

### 6.4 Help 文本更新

```
auto <需求>        自动推进 SDD Loop
auto --resume      恢复当前 auto run
auto --restart     重启 auto run
auto --stop        停止当前 auto run
auto --events      查看 auto run 事件
auto --status      查看 auto run 状态
```

**涉及测试用例（M4）**：AC-009, AC-019, AC-025, AC-026, AC-027, AC-028, AC-029, AC-030, AC-031

---

## 7. Milestone 5：Result v2 与集成测试

### 7.1 v2 result 处理策略

选择**方案 B**：v2 result 必须包含 `legacy` TDD evidence，否则返回 `E_TDD_EVIDENCE_REQUIRED`。与现有 `toLegacyResult()` 行为一致。

`build complete` 中增加 v2 result 判断分支（检查 `schemaVersion === "1.2.0"` 且 `fileDelta` 存在），提取 `legacy` 后继续现有流程。

如果后续需要正规化（方案 A），可在 `task-result-normalizer.ts` 中增加 v2→v1 转换函数。

### 7.2 集成测试补齐

重点覆盖：

1. `init → auto → build next → build complete → auto --resume → archive` 端到端
2. Schema 迁移幂等性
3. CLI golden output 测试
4. `auto --restart / --stop / --events / --status` 行为

**涉及测试用例（M5）**：AC-032, AC-034

---

## 8. 风险与注意事项

### 8.1 向后兼容

- Schema 1.2.0 → 1.3.0 迁移后，旧版 CLI 读取新版 state 会抛出 `E_STATE_CORRUPTED`。迁移日志已覆盖此场景。
- `StateStore.read()` 已有迁移框架（1.0.0 → 1.2.0），1.3.0 需增加 BUILDING 等待状态检测逻辑。

### 8.2 并发安全

- `build next/complete`、`auto --resume/restart/stop` 持有 FileLock，保证串行化。
- 只读命令（`status`, `status --loop`, `auto --events`, `auto --status`）不持锁。

### 8.3 测试兼容

- Windows CI 上部分测试需跳过（遵循现有 `process.platform === "win32" ? it.skip` 模式）。
- 临时目录测试沿用 `tmpdir()` + `roots[]` + `afterEach` 清理模式。

### 8.4 LoopEngine 复用

- `LoopEngine` 每次 `runAuto()` 构造新实例，开销轻量（只是组合现有 Store + EventStore），不引入全局状态。

---

## 9. 验收摘要

| AC         | 描述                                                  | Milestone |
| ---------- | ----------------------------------------------------- | --------- |
| AC-001     | 不新增 `loop` 命令                                    | M3        |
| AC-002-005 | Auto 作为唯一 Loop 入口，可从各阶段推进               | M3        |
| AC-006-008 | `build next` 返回 BUILD_WAITING_AGENT，重复不重复分配 | M1        |
| AC-009     | `auto --resume` 在等待 Agent 时返回同一 handoff       | M4        |
| AC-010-014 | `build complete` 安全校验                             | M2        |
| AC-015-016 | 任务完成后正确切换到 BUILD_READY / PLAN_READY         | M1        |
| AC-017-018 | design/plan unchanged 状态收敛                        | M1        |
| AC-019     | 文本输出显示 actionRequired                           | M4        |
| AC-020     | PLAN_READY 的 next 为 `sdd build next`                | M1        |
| AC-021     | codebase 信息从 state 读取                            | M2        |
| AC-022-024 | LoopStep / Event 记录                                 | M3        |
| AC-025-027 | `auto --events/--status` 和 `status --loop`           | M4        |
| AC-028-030 | verify/review 失败后 auto 停止；archive 成功完成 run  | M4        |
| AC-031     | `auto --stop` 不破坏 change                           | M4        |
| AC-032     | Schema 迁移幂等                                       | M5        |
| AC-033     | 旧 BUILDING 等待状态迁移                              | M1        |
| AC-034     | v2 result 行为明确                                    | M5        |
