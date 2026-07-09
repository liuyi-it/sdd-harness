# Auto Loop Engine 正式化 — 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `sdd auto` 内部升级为可恢复、可观测、可决策的 SDD Loop Engine，不新增 `sdd loop` 公开命令。

**Architecture:** 5 个 Milestone 逐层推进。M1 稳定 Handoff 状态机（新增 BUILD_WAITING_AGENT phase），M2 修复安全校验漏洞，M3 抽离 LoopEngine/DecisionEngine/EventStore，M4 增强 CLI 命令和输出，M5 补齐 result v2 契约和集成测试。

**Tech Stack:** TypeScript ESM, Node.js, Vitest, Zod (state validation), 现有 Core/CLI 架构

## Global Constraints

- 不新增 `sdd loop` 公开命令
- 不绕过任何单阶段命令的校验
- 所有状态推进只能由 Core 命令完成
- Schema 版本从 1.2.0 升级到 1.3.0
- 代码以中文注释为主，错误码使用 `E_*` 英文标识符
- 每次改动需运行 `npm run format:check && npm run lint && npm run typecheck && npm test`

---

## Milestone 1：Handoff 状态稳定化

### Task 1.1: 新增 BUILD_WAITING_AGENT Phase 到 contracts

**Files:**
- Modify: `packages/core/src/contracts.ts`

**Interfaces:**
- Consumes: 现有 `PHASES` 数组
- Produces: `PHASES` 包含 `"BUILD_WAITING_AGENT"`，`Phase` 类型自动扩展

- [ ] **Step 1: 在 PHASES 数组中新增 BUILD_WAITING_AGENT**

在 `packages/core/src/contracts.ts` 的 `PHASES` 数组中，`"BUILDING"` 之后新增 `"BUILD_WAITING_AGENT"`：

```ts
export const PHASES = [
  "NOT_INITIALIZED",
  "INITIALIZING",
  "INDEXING",
  "INDEX_READY",
  "NEW_STARTED",
  "CLARIFYING",
  "SPEC_READY",
  "DESIGNING",
  "DESIGN_READY",
  "PLANNING",
  "PLAN_READY",
  "BUILDING",
  "BUILD_WAITING_AGENT",  // ← 新增
  "BUILD_READY",
  "VERIFYING",
  "VERIFY_READY",
  "REVIEWING",
  "REVIEW_READY",
  "ARCHIVING",
  "ARCHIVED",
  "FAILED",
  "PAUSED",
] as const;
```

- [ ] **Step 2: 运行类型检查确认无编译错误**

```bash
npm run typecheck
```

- [ ] **Step 3: 提交**

```bash
git add packages/core/src/contracts.ts
git commit -m "feat: contracts PHASES 新增 BUILD_WAITING_AGENT 阶段"
```

---

### Task 1.2: 修改 build next 返回 BUILD_WAITING_AGENT

**Files:**
- Modify: `packages/core/src/commands/build.ts` (buildNextTask 函数)

**Interfaces:**
- Consumes: `StateStore`, `BUILD_WAITING_AGENT` phase
- Produces: `buildNextTask()` 返回 `state: "BUILD_WAITING_AGENT"`，写入 `activeLoop.waiting`

- [ ] **Step 1: 修改 StateStore.update() 调用中的状态写入**

定位到 `packages/core/src/commands/build.ts` 的 `buildNextTask()` 函数中 `await new StateStore(root).update(...)` 调用（约第 745 行）。

当前代码：
```ts
await new StateStore(root).update((current) => ({
  ...current,
  currentPhase: "BUILDING",
  inProgressPhase: "BUILDING",
  lastCommand: "sdd build next",
  lastError: null,
  suggestedCommand: "sdd build next",
  tasks: { ...current.tasks, [nextTask.id]: "BUILDING" },
}));
```

替换为：
```ts
await new StateStore(root).update((current) => ({
  ...current,
  currentPhase: "BUILD_WAITING_AGENT",
  inProgressPhase: null,
  lastCommand: "sdd build next",
  lastError: null,
  suggestedCommand: "sdd build complete",
  tasks: { ...current.tasks, [nextTask.id]: "BUILDING" },
  activeLoop: current.activeLoop !== null && typeof current.activeLoop === "object"
    ? {
        ...(current.activeLoop as Record<string, unknown>),
        status: "WAITING_AGENT",
        waiting: {
          reason: "AGENT_TASK_EXECUTION",
          taskId: nextTask.id,
          resultFile,
          since: new Date().toISOString(),
        },
      }
    : current.activeLoop,
}));
```

- [ ] **Step 2: 修改返回值的 state 字段**

将同函数末尾 return 中的 `state: "BUILDING"` 改为 `state: "BUILD_WAITING_AGENT"`（约第 777 行）：

```ts
return {
  ok: true,
  state: "BUILD_WAITING_AGENT",  // 从 "BUILDING" 改为 "BUILD_WAITING_AGENT"
  exitCode: 0,
  actionRequired,
  next: "sdd build complete",
};
```

- [ ] **Step 3: 运行类型检查**

```bash
npm run typecheck
```
Expected: PASS，无类型错误。

- [ ] **Step 4: 更新 build.test.ts 中相关断言**

在 `packages/core/test/build.test.ts` 中，查找 `state: "BUILDING"` 的断言，将涉及 `build next` 的断言改为 `state: "BUILD_WAITING_AGENT"`.

- [ ] **Step 5: 运行测试**

```bash
npm test
```
Expected: 全部通过。

- [ ] **Step 6: 提交**

```bash
git add packages/core/src/commands/build.ts packages/core/test/build.test.ts
git commit -m "feat: build next 返回 BUILD_WAITING_AGENT 状态，写入 activeLoop.waiting"
```

---

### Task 1.3: 重复 build next 不重复分配任务

**Files:**
- Modify: `packages/core/src/commands/build.ts` (buildNextTask 函数开头)

**Interfaces:**
- Consumes: 现有 `buildNextTask()` 逻辑
- Produces: 当处于 BUILD_WAITING_AGENT 时返回同一 handoff

- [ ] **Step 1: 在 buildNextTask() 开头增加等待状态检查**

在 `buildNextTask()` 中 FileLock 获取之后、现有逻辑之前，增加：

```ts
// 如果当前已有 active waiting task，返回同一份 handoff，不重复分配
if (state.currentPhase === "BUILD_WAITING_AGENT") {
  const activeLoop = state.activeLoop as Record<string, unknown> | null;
  const waiting = activeLoop?.waiting as Record<string, unknown> | undefined;
  if (waiting?.taskId && waiting?.resultFile) {
    const existingTaskId = waiting.taskId as string;
    const existingResultFile = waiting.resultFile as string;
    const changeId = requireActiveChangeId(state.currentChangeId, rawArgs);
    const change = join(root, ".sdd", "changes", changeId);
    const tasks = JSON.parse(
      await readFile(join(change, "tasks.json"), "utf8"),
    ) as TaskDefinition[];
    const task = tasks.find((t) => t.id === existingTaskId);
    if (task) {
      const contextPackPath = `.sdd/context-packs/${changeId}/${existingTaskId}.md`;
      // 确保 result 目录存在
      await mkdir(join(root, ".sdd", "runs", state.currentRunId ?? "unknown-run", "tasks"), {
        recursive: true,
      });
      const actionRequired: AgentActionRequired = {
        type: "AGENT_TASK_EXECUTION",
        taskId: existingTaskId,
        changeId,
        contextPack: contextPackPath,
        allowedFiles: task.allowedFiles ?? [],
        expectedNewFiles: task.expectedNewFiles ?? [],
        forbiddenFiles: task.forbiddenFiles ?? [],
        verification:
          task.verification?.map((cmd: string) => {
            const [command, ...rest] = cmd.split(/\s+/);
            return { command: command!, args: rest };
          }) ?? [],
        resultFile: existingResultFile,
        codebase: {
          provider: state.codebaseProvider === "codebase-memory-mcp"
            ? "codebase-memory-mcp" as const
            : "fallback-file-scan" as const,
          degraded: state.degraded,
        },
      };
      return {
        ok: true,
        state: "BUILD_WAITING_AGENT",
        exitCode: 0,
        actionRequired,
        next: "sdd build complete",
      };
    }
  }
}
```

- [ ] **Step 2: 运行类型检查**

```bash
npm run typecheck
```

- [ ] **Step 3: 编写测试：重复 build next 返回一致 handoff**

在 `packages/core/test/build.test.ts` 中新增测试：
```ts
it("重复 build next 不重复分配任务，返回同一 taskId/contextPack/resultFile", async () => {
  // 设置状态为 PLAN_READY + tasks.json 包含待执行任务
  // 第一次 build next → 获取 taskId
  // 第二次 build next → taskId 相同
});
```

- [ ] **Step 4: 运行测试**

```bash
npm test
```

- [ ] **Step 5: 提交**

```bash
git add packages/core/src/commands/build.ts packages/core/test/build.test.ts
git commit -m "feat: build next 在 BUILD_WAITING_AGENT 时返回同一 handoff，不重复分配"
```

---

### Task 1.4: 修改 build complete 完成后状态

**Files:**
- Modify: `packages/core/src/commands/build.ts` (buildCompleteTask 末尾 state.update)

**Interfaces:**
- Consumes: 当前 `buildCompleteTask()` 状态更新逻辑
- Produces: 还有任务→PLAN_READY，全部完成→BUILD_READY，清除 activeLoop.waiting

- [ ] **Step 1: 修改状态写入逻辑**

定位到 `buildCompleteTask()` 末尾的 `store.update()` 调用（约第 986 行）。

当前代码：
```ts
await store.update((current) => ({
  ...current,
  tasks: { ...current.tasks, [taskId]: taskStatus },
  currentPhase: allDone ? ("BUILD_READY" as const) : ("BUILDING" as const),
  inProgressPhase: null,
  failedCommand: null,
  failedReason: null,
  interruptedCommand: null,
  lastCommand: "sdd build complete",
  lastError: null,
  suggestedCommand: allDone ? "sdd verify" : "sdd build next",
}));
```

替换为：
```ts
await store.update((current) => ({
  ...current,
  tasks: { ...current.tasks, [taskId]: taskStatus },
  currentPhase: allDone ? ("BUILD_READY" as const) : ("PLAN_READY" as const),
  inProgressPhase: null,
  failedCommand: null,
  failedReason: null,
  interruptedCommand: null,
  lastCommand: "sdd build complete",
  lastError: null,
  suggestedCommand: allDone ? "sdd verify" : "sdd build next",
  // 清除 activeLoop.waiting
  activeLoop: current.activeLoop !== null && typeof current.activeLoop === "object"
    ? (() => {
        const loop = { ...(current.activeLoop as Record<string, unknown>) };
        loop.status = "RUNNING";
        delete loop.waiting;
        return loop;
      })()
    : current.activeLoop,
}));
```

- [ ] **Step 2: 修改返回值的 state 字段**

同函数末尾 return 中的 `state: allDone ? "BUILD_READY" : "BUILDING"` 改为：

```ts
state: allDone ? "BUILD_READY" : "PLAN_READY",
```

- [ ] **Step 3: 运行类型检查**

```bash
npm run typecheck
```

- [ ] **Step 4: 更新 build.test.ts 相关断言**

查找 `build complete` 后返回 `BUILDING` 的断言，改为 `PLAN_READY`。

- [ ] **Step 5: 运行测试**

```bash
npm test
```

- [ ] **Step 6: 提交**

```bash
git add packages/core/src/commands/build.ts packages/core/test/build.test.ts
git commit -m "fix: build complete 完成后正确切换到 PLAN_READY 而非 BUILDING"
```

---

### Task 1.5: 修复 design/plan unchanged 状态收敛

**Files:**
- Modify: `packages/core/src/commands/design.ts` (unchanged 早退逻辑)
- Modify: `packages/core/src/commands/plan.ts` (unchanged 早退逻辑)

**Interfaces:**
- Consumes: 现有 `runDesign()` / `runPlan()` unchanged 分支
- Produces: unchanged 时写回 state.json，不再不一致

- [ ] **Step 1: 修复 design unchanged**

定位到 `packages/core/src/commands/design.ts` 第 103-112 行。

当前代码：
```ts
if (unchanged) {
  return {
    ok: true,
    state: "DESIGN_READY",
    exitCode: 0,
    changeId,
    next: "sdd plan",
    data: { alreadyReady: true },
  };
}
```

替换为：
```ts
if (unchanged) {
  await store.update((current) => ({
    ...current,
    currentPhase: "DESIGN_READY",
    inProgressPhase: null,
    suggestedCommand: "sdd plan",
    artifacts: { ...current.artifacts, design: "READY" },
  }));
  return {
    ok: true,
    state: "DESIGN_READY",
    exitCode: 0,
    changeId,
    next: "sdd plan",
    data: { alreadyReady: true },
  };
}
```

- [ ] **Step 2: 修复 plan unchanged**

定位到 `packages/core/src/commands/plan.ts` 第 124-133 行。

当前代码：
```ts
if (unchanged) {
  return {
    ok: true,
    state: "PLAN_READY",
    exitCode: 0,
    changeId,
    next: "sdd build",
    data: { alreadyReady: true },
  };
}
```

替换为：
```ts
if (unchanged) {
  await store.update((current) => ({
    ...current,
    currentPhase: "PLAN_READY",
    inProgressPhase: null,
    suggestedCommand: "sdd build next",
    artifacts: {
      ...current.artifacts,
      tasks: "READY" as const,
      testPlan: "READY" as const,
      context: "READY" as const,
    },
  }));
  return {
    ok: true,
    state: "PLAN_READY",
    exitCode: 0,
    changeId,
    next: "sdd build next",
    data: { alreadyReady: true },
  };
}
```

- [ ] **Step 3: 运行类型检查**

```bash
npm run typecheck
```

- [ ] **Step 4: 在 design-plan.test.ts 中新增测试**

```ts
it("design unchanged 时必须写回 DESIGN_READY 到 state.json", async () => {
  // 执行 design 两次，第二次应该 unchanged
  // 验证 state.json 中 currentPhase === "DESIGN_READY"
});

it("plan unchanged 时必须写回 PLAN_READY 到 state.json", async () => {
  // 同理
});
```

- [ ] **Step 5: 运行测试**

```bash
npm test
```

- [ ] **Step 6: 提交**

```bash
git add packages/core/src/commands/design.ts packages/core/src/commands/plan.ts packages/core/test/design-plan.test.ts
git commit -m "fix: design/plan unchanged 时写回收敛后的 state，消除输出与持久化不一致"
```

---

### Task 1.6: 修复 PLAN_READY 的 status next

**Files:**
- Modify: `packages/core/src/commands/status.ts` (NEXT_BY_PHASE)
- Modify: `packages/core/src/state/state-store.ts` (suggestedCommand 函数)

- [ ] **Step 1: 修改 status.ts 中的 NEXT_BY_PHASE**

在 `packages/core/src/commands/status.ts` 第 60 行：

```ts
// 从:
PLAN_READY: "sdd build",
// 改为:
PLAN_READY: "sdd build next",
```

- [ ] **Step 2: 修改 state-store.ts 中的 suggestedCommand**

在 `packages/core/src/state/state-store.ts` 第 431 行：

```ts
// 从:
PLAN_READY: "sdd build",
// 改为:
PLAN_READY: "sdd build next",
```

- [ ] **Step 3: 更新相关测试断言**

在 `state.test.ts` 或相关测试中，查找 `"sdd build"` 且上下文涉及 PLAN_READY 的断言，改为 `"sdd build next"`。

- [ ] **Step 4: 运行测试**

```bash
npm test
```

- [ ] **Step 5: 提交**

```bash
git add packages/core/src/commands/status.ts packages/core/src/state/state-store.ts packages/core/test/state.test.ts
git commit -m "fix: PLAN_READY 的 next 从 sdd build 改为 sdd build next"
```

---

### Task 1.7: Schema 迁移 1.2.0 → 1.3.0

**Files:**
- Modify: `packages/core/src/state/schema-migration.ts`
- Modify: `packages/core/src/state/state-store.ts` (read 方法增加 1.2.0→1.3.0 迁移)

- [ ] **Step 1: 升级 CURRENT_SCHEMA_VERSION**

在 `packages/core/src/state/schema-migration.ts` 中：

```ts
// 从:
export const CURRENT_SCHEMA_VERSION = "1.2.0";
// 改为:
export const CURRENT_SCHEMA_VERSION = "1.3.0";
```

- [ ] **Step 2: 在 StateStore.read() 中增加 1.2.0→1.3.0 迁移逻辑**

在 `packages/core/src/state/state-store.ts` 的 `read()` 方法中，`schemaVersion` 校验之前（约第 170 行），增加：

```ts
// 1.2.0 → 1.3.0：检测旧 BUILDING 等待状态并迁移
if (raw.schemaVersion === "1.2.0") {
  const migrated = this.migrateFrom120(raw);
  await this.write(migrated);
  return migrated;
}
```

在 StateStore 类中新增 `migrateFrom120` 方法：

```ts
private migrateFrom120(raw: Record<string, unknown>): WorkflowState {
  const phase = raw.currentPhase as string | undefined;
  const suggested = raw.suggestedCommand as string | undefined;
  const tasks = (raw.tasks as Record<string, string>) ?? {};

  if (phase === "BUILDING" && suggested?.includes("build next")) {
    // 找到 BUILDING 状态的任务
    const buildingTasks = Object.entries(tasks).filter(
      ([, status]) => status === "BUILDING"
    );
    if (buildingTasks.length === 1) {
      const [taskId] = buildingTasks[0]!;
      return workflowStateSchema.parse({
        ...raw,
        schemaVersion: "1.3.0",
        currentPhase: "BUILD_WAITING_AGENT",
        inProgressPhase: null,
        activeLoop: {
          loopId: "auto-default",
          runId: (raw.currentRunId as string) ?? `run-${Date.now()}`,
          status: "WAITING_AGENT",
          waiting: {
            reason: "AGENT_TASK_EXECUTION",
            taskId,
            since: (raw.updatedAt as string) ?? new Date().toISOString(),
          },
        },
        version: typeof raw.version === "number" ? raw.version + 1 : 1,
        updatedAt: new Date().toISOString(),
      });
    }
    // 无法确定唯一 taskId → 标记 FAILED
    return workflowStateSchema.parse({
      ...raw,
      schemaVersion: "1.3.0",
      currentPhase: "FAILED",
      failedReason: "旧 BUILDING 等待状态迁移失败：无法确定唯一 taskId，请人工检查",
    });
  }

  // 其他 1.2.0 → 1.3.0：仅升级 schemaVersion（无需重建 activeLoop）
  return workflowStateSchema.parse({
    ...raw,
    schemaVersion: "1.3.0",
    version: typeof raw.version === "number" ? raw.version + 1 : 1,
    updatedAt: new Date().toISOString(),
  });
}
```

- [ ] **Step 3: 同时更新 workflowStateSchema 的 schemaVersion**

在 `packages/core/src/state/state-store.ts` 的 `workflowStateSchema` 中将 `schemaVersion` 从 `z.literal("1.2.0")` 升级到：

```ts
schemaVersion: z.enum(["1.2.0", "1.3.0"]),
```

（因为迁移前 state 仍可能是 1.2.0 且尚未迁移，需要放宽校验）

实际上，由于迁移逻辑在 `read()` 中先于 parse 执行，读到的 state 已经是 1.3.0 了。但 backup recovery 或 recoverFromArtifacts 可能直接使用 `workflowStateSchema.parse()` 创建新状态——那是 `createInitialState()` 返回的 ，已经是 `CURRENT_SCHEMA_VERSION` 了。

所以更干净的方案是：在 `read()` 中增加 `if (raw.schemaVersion === "1.2.0")` 的提前处理分支，在 `workflowStateSchema.parse()` 之前迁移。这样 `workflowStateSchema` 保持 `z.literal("1.3.0")` 即可。

- [ ] **Step 4: 运行类型检查**

```bash
npm run typecheck
```

- [ ] **Step 5: 编写迁移测试**

在 `packages/core/test/state.test.ts` 中：

```ts
it("1.2.0 BUILDING 等待状态迁移为 BUILD_WAITING_AGENT", async () => {
  // 创建 1.2.0 state：currentPhase=BUILDING, suggestedCommand="sdd build next", tasks={"TASK-1": "BUILDING"}
  // 调用 StateStore.read()
  // 验证迁移后 currentPhase === "BUILD_WAITING_AGENT"
  // 验证 activeLoop.status === "WAITING_AGENT"
});

it("1.2.0 不确定 taskId 时迁移为 FAILED", async () => {
  // 多个 BUILDING 任务
  // 验证迁移后 currentPhase === "FAILED"
});

it("schema 迁移可重复执行不损坏 state", async () => {
  // 已迁移到 1.3.0 的 state，再次 read() 不重复迁移
});
```

- [ ] **Step 6: 运行测试**

```bash
npm test
```

- [ ] **Step 7: 提交**

```bash
git add packages/core/src/state/schema-migration.ts packages/core/src/state/state-store.ts packages/core/test/state.test.ts
git commit -m "feat: schema 1.2.0→1.3.0 迁移，支持旧 BUILDING 等待状态转 BUILD_WAITING_AGENT"
```

---

## Milestone 2：基础安全修复

### Task 2.1: build complete taskId 一致性校验

**Files:**
- Modify: `packages/core/src/commands/build.ts` (buildCompleteTask)

**Interfaces:**
- Consumes: `buildCompleteTask` 现有逻辑
- Produces: taskId 不一致时返回错误

- [ ] **Step 1: 增加 taskId 校验**

在 `buildCompleteTask()` 中 `resultJson` 读取后（约第 834 行之后），增加：

```ts
// taskId 一致性校验
if (resultJson.taskId !== taskId) {
  return {
    ok: false,
    state: "FAILED",
    exitCode: 4,
    error: {
      code: "E_STATE_CORRUPTED",
      message: `result.taskId (${String(resultJson.taskId)}) 与 --task (${taskId}) 不一致`,
    },
  };
}
```

- [ ] **Step 2: 编写测试**

```ts
it("build complete 拒绝 taskId 不一致的 result", async () => {
  // --task TASK-A 但 result.taskId === "TASK-B"
  // 验证返回 E_STATE_CORRUPTED
});
```

- [ ] **Step 3: 运行测试**

```bash
npm test
```

- [ ] **Step 4: 提交**

```bash
git add packages/core/src/commands/build.ts packages/core/test/build.test.ts
git commit -m "fix: build complete 校验 result.taskId 与 --task 一致"
```

---

### Task 2.2: build complete 当前等待任务校验

**Files:**
- Modify: `packages/core/src/commands/build.ts` (buildCompleteTask)

- [ ] **Step 1: 增加等待任务校验**

在 `StateStore.read()` 之后、现有逻辑之前，增加：

```ts
// 如果处于 BUILD_WAITING_AGENT，校验 complete 的是当前等待任务
if (state.currentPhase === "BUILD_WAITING_AGENT") {
  const activeLoop = state.activeLoop as Record<string, unknown> | null;
  const waiting = activeLoop?.waiting as Record<string, unknown> | undefined;
  if (waiting?.taskId && waiting.taskId !== taskId) {
    return {
      ok: false,
      state: "FAILED",
      exitCode: 4,
      error: {
        code: "E_STATE_CORRUPTED",
        message: `当前等待任务为 ${String(waiting.taskId)}，不能 complete ${taskId}`,
      },
    };
  }
}
```

- [ ] **Step 2: 编写测试**

```ts
it("build complete 拒绝 complete 非当前等待任务", async () => {
  // activeLoop.waiting.taskId === "TASK-1"，但 --task TASK-2
  // 验证返回 E_STATE_CORRUPTED
});
```

- [ ] **Step 3: 运行测试并提交**

```bash
npm test
git add packages/core/src/commands/build.ts packages/core/test/build.test.ts
git commit -m "fix: build complete 校验只能 complete 当前等待任务"
```

---

### Task 2.3: build complete 改用 validateTaskFiles

**Files:**
- Modify: `packages/core/src/commands/build.ts` (buildCompleteTask)

- [ ] **Step 1: 删除旧 out-of-scope 逻辑，改用 validateTaskFiles**

删除 `buildCompleteTask()` 第 867-886 行的旧 out-of-scope 判断逻辑：

```ts
// 删除这段旧代码：
const outOfScope = modifiedFiles.filter(
  (f) =>
    !allowedFiles.includes(f) &&
    !expectedNewFiles.includes(f) &&
    forbiddenFiles.some((pf) => f === pf || f.startsWith(pf + "/")),
);
if (outOfScope.length > 0) { ... }
```

替换为统一调用 `validateTaskFiles`：

```ts
// 文件范围校验：统一使用 validateTaskFiles
try {
  validateTaskFiles(modifiedFiles, {
    allowedFiles: task.allowedFiles ?? [],
    expectedNewFiles: task.expectedNewFiles ?? [],
    forbiddenFiles: task.forbiddenFiles ?? [],
  });
} catch (error) {
  if (error instanceof SddError && error.code === "E_SECURITY_BLOCKED") {
    return {
      ok: false,
      state: "FAILED",
      exitCode: 5,
      error: { code: "E_SECURITY_BLOCKED", message: error.message },
    };
  }
  throw error;
}
```

注意：`validateTaskFiles` 已经 import 在文件顶部（第 27 行），无需新增 import。

- [ ] **Step 2: 同时清理未使用的局部变量**

删除 `buildCompleteTask()` 中旧的 `allowedFiles`、`expectedNewFiles`、`forbiddenFiles` 局部变量（如果它们仅用于旧的 out-of-scope 逻辑）。

- [ ] **Step 3: 编写测试**

```ts
it("build complete validateTaskFiles 阻断越权文件", async () => {
  // task.allowedFiles = ["src/a.ts"]，result 声明修改 "src/b.ts"
  // 验证返回 E_SECURITY_BLOCKED
});

it("build complete validateTaskFiles 阻断 forbidden 文件", async () => {
  // task.forbiddenFiles = [".env"]，result 声明修改 ".env"
  // 验证返回 E_SECURITY_BLOCKED
});
```

- [ ] **Step 4: 运行测试并提交**

```bash
npm test
git add packages/core/src/commands/build.ts packages/core/test/build.test.ts
git commit -m "fix: build complete 统一使用 validateTaskFiles 进行文件范围校验"
```

---

### Task 2.4: TDD evidence 命令安全校验加强

**Files:**
- Modify: `packages/core/src/commands/build.ts` (buildCompleteTask)

- [ ] **Step 1: 在 evidence 校验中增加命令安全检查**

在 `buildCompleteTask()` 的 TDD evidence 校验部分（约第 890 行之后），增加命令安全检查：

```ts
// 命令安全校验
const blockedEvidence = tddEvidence.find(
  (e: Record<string, unknown>) =>
    typeof e.command === "string" && !isCommandAllowed(e.command),
);
if (blockedEvidence) {
  return {
    ok: false,
    state: "FAILED",
    exitCode: 5,
    error: {
      code: "E_SECURITY_BLOCKED",
      message: `TDD 证据命令未在允许清单内：${String(blockedEvidence.command)}`,
    },
  };
}
```

同样对 `verification` 数组增加命令安全检查（当前已有部分，确认完整）。

- [ ] **Step 2: 确认 isCommandAllowed 已 import**

文件顶部已 import（第 26 行 `isCommandAllowed` 来自 `shell-policy`？），如果没有则新增 import：
```ts
import { isCommandAllowed } from "../security/shell-policy.js";
```

（检查 `build.ts` 第 26 行：已有 `isCommandAllowed` 使用在 `validateExecution()` 中，确认可用。）

- [ ] **Step 3: 编写测试**

```ts
it("build complete 阻断安全清单外的 TDD evidence 命令", async () => {
  // tddEvidence 包含 command: "rm -rf /"
  // 验证返回 E_SECURITY_BLOCKED
});
```

- [ ] **Step 4: 运行测试并提交**

```bash
npm test
git add packages/core/src/commands/build.ts packages/core/test/build.test.ts
git commit -m "fix: build complete 加强 TDD evidence/verification 命令安全校验"
```

---

### Task 2.5: AgentActionRequired.codebase 从 state 读取

**Files:**
- Modify: `packages/core/src/commands/build.ts` (buildNextTask)

- [ ] **Step 1: 修改 codebase 构建逻辑**

在 `buildNextTask()` 中构建 `actionRequired` 的位置（约第 769 行），将硬编码的 codebase 改为从 state 读取：

```ts
// 从:
codebase: {
  provider: "fallback-file-scan",
  degraded: true,
},

// 改为:
codebase: {
  provider: state.codebaseProvider === "codebase-memory-mcp"
    ? "codebase-memory-mcp" as const
    : "fallback-file-scan" as const,
  degraded: state.degraded,
},
```

- [ ] **Step 2: 编写测试**

```ts
it("build next 的 codebase 信息来自 state", async () => {
  // state.codebaseProvider = "codebase-memory-mcp", state.degraded = false
  // 验证 actionRequired.codebase.provider === "codebase-memory-mcp"
  // 验证 actionRequired.codebase.degraded === false
});
```

- [ ] **Step 3: 运行测试并提交**

```bash
npm test
git add packages/core/src/commands/build.ts packages/core/test/build.test.ts
git commit -m "fix: AgentActionRequired.codebase 从 state 读取而非硬编码 fallback"
```

---

## Milestone 3：Auto LoopEngine 抽象

### Task 3.1: 升级 Loop 数据模型到 1.3.0

**Files:**
- Modify: `packages/core/src/loop/model.ts`
- Modify: `packages/core/src/loop/loop-spec.ts`

- [ ] **Step 1: 升级 model.ts 接口定义**

```ts
// LoopSpec
export interface LoopSpec {
  schemaVersion: "1.3.0";
  loopId: string;
  mode: "auto";
  maxSteps: number;
  maxRetriesPerStep: number;
  maxRepeatedFailures: number;
  stoppingRules: string[];
  decisionPolicy: LoopDecisionPolicy; // 见下方
  createdAt: string;
  updatedAt: string;
}

export type LoopDecisionPolicy = "STRICT" | "BALANCED";

// ActiveLoop
export interface ActiveLoop {
  loopId: string;
  runId: string;
  status: "RUNNING" | "WAITING_AGENT" | "PAUSED" | "FAILED" | "SUCCEEDED" | "ABORTED";
  waiting?: {
    reason: "AGENT_TASK_EXECUTION" | "CLARIFICATION" | "HUMAN_REVIEW";
    taskId?: string;
    resultFile?: string;
    since: string;
  };
  recovered?: boolean;
}

// LoopStep
export interface LoopStep {
  step: number;
  kind: "COMMAND" | "AGENT_HANDOFF" | "DECISION" | "VERIFY" | "REVIEW" | "ARCHIVE";
  command: string;
  phaseBefore: string;
  phaseAfter?: string;
  status: "SUCCEEDED" | "FAILED" | "BLOCKED" | "SKIPPED" | "PAUSED" | "WAITING_AGENT";
  decision?: LoopDecision;
  actionRequired?: import("../contracts.js").AgentActionRequired;
  error?: import("../contracts.js").CommandError;
  warnings?: Array<string | import("../contracts.js").CliWarning>;
  artifacts?: string[];
  startedAt: string;
  endedAt: string;
}

// LoopDecision
export type LoopDecision =
  | "CONTINUE"
  | "PAUSE_FOR_AGENT"
  | "PAUSE_FOR_CLARIFICATION"
  | "PAUSE_FOR_HUMAN"
  | "FAIL"
  | "ABORT"
  | "DONE";

// LoopWaitingState
export interface LoopWaitingState {
  reason: "AGENT_TASK_EXECUTION" | "CLARIFICATION" | "HUMAN_REVIEW";
  taskId?: string;
  resultFile?: string;
  since: string;
}

// LoopRun
export interface LoopRun {
  schemaVersion: "1.3.0";
  runId: string;
  loopId: string;
  changeId?: string;
  status: "PENDING" | "RUNNING" | "WAITING_AGENT" | "PAUSED" | "SUCCEEDED" | "FAILED" | "ABORTED" | "ARCHIVED";
  startedAt: string;
  updatedAt: string;
  endedAt?: string;
  currentStep: number;
  lastDecision?: LoopDecision;
  waiting?: LoopWaitingState;
  steps: LoopStep[];
}
```

- [ ] **Step 2: 升级 loop-spec.ts**

```ts
export function createDefaultLoopSpec(): LoopSpec {
  const now = new Date().toISOString();
  return {
    schemaVersion: "1.3.0",
    loopId: "auto-default",
    mode: "auto",
    maxSteps: 12,
    maxRetriesPerStep: 0,
    maxRepeatedFailures: 2,
    stoppingRules: [
      "CLARIFYING",
      "WAITING_AGENT",
      "VERIFY_FAILED",
      "REVIEW_FAILED",
      "SECURITY_BLOCKED",
      "STATE_CORRUPTED",
    ],
    decisionPolicy: "BALANCED",
    createdAt: now,
    updatedAt: now,
  };
}
```

- [ ] **Step 3: 更新所有使用旧 model 的代码（auto.ts, loop.test.ts 等）**

将 `schemaVersion: "1.2.0"` 引用改为 `"1.3.0"`，处理新增必填字段。

- [ ] **Step 4: 运行类型检查和测试**

```bash
npm run typecheck && npm test
```

- [ ] **Step 5: 提交**

```bash
git add packages/core/src/loop/model.ts packages/core/src/loop/loop-spec.ts packages/core/src/commands/auto.ts packages/core/test/loop.test.ts packages/core/test/auto.test.ts
git commit -m "feat: Loop 数据模型升级到 1.3.0，新增 decision/waiting/event 字段"
```

---

### Task 3.2: 新增 LoopDecisionEngine

**Files:**
- Create: `packages/core/src/loop/loop-decision.ts`
- Create: `packages/core/test/loop-decision.test.ts`

- [ ] **Step 1: 创建 loop-decision.ts**

```ts
import type { CommandResult, Phase } from "../contracts.js";
import type { LoopDecision } from "./model.js";

/**
 * DecisionEngine：纯函数，根据 CommandResult 和当前 Phase 决策下一步动作。
 * 规则见 docs/四期需求文档.md §11.2
 */
export function decide(input: {
  result: CommandResult;
}): LoopDecision {
  if (!input.result.ok) {
    if (input.result.error?.code === "E_VERIFY_FAILED") return "PAUSE_FOR_HUMAN";
    if (input.result.error?.code === "E_REVIEW_FAILED") return "PAUSE_FOR_HUMAN";
    if (input.result.error?.code === "E_SECURITY_BLOCKED") return "FAIL";
    if (input.result.error?.code === "E_STATE_CORRUPTED") return "FAIL";
    return "FAIL";
  }

  const state = input.result.state as Phase;

  if (state === "CLARIFYING") return "PAUSE_FOR_CLARIFICATION";
  if (state === "BUILD_WAITING_AGENT") return "PAUSE_FOR_AGENT";
  if (input.result.actionRequired?.type === "AGENT_TASK_EXECUTION") return "PAUSE_FOR_AGENT";

  if (state === "BUILD_READY") return "CONTINUE";
  if (state === "VERIFY_READY") return "CONTINUE";
  if (state === "REVIEW_READY") return "CONTINUE";

  if (state === "ARCHIVED") return "DONE";

  return "CONTINUE";
}
```

- [ ] **Step 2: 创建测试**

```ts
// loop-decision.test.ts
import { describe, expect, it } from "vitest";
import { decide } from "../src/loop/loop-decision.js";

describe("DecisionEngine", () => {
  it("ok=false → FAIL", () => {
    expect(decide({ result: { ok: false, state: "PLAN_READY", exitCode: 1 } })).toBe("FAIL");
  });

  it("CLARIFYING → PAUSE_FOR_CLARIFICATION", () => {
    expect(decide({ result: { ok: true, state: "CLARIFYING", exitCode: 0 } })).toBe("PAUSE_FOR_CLARIFICATION");
  });

  it("BUILD_WAITING_AGENT → PAUSE_FOR_AGENT", () => {
    expect(decide({ result: { ok: true, state: "BUILD_WAITING_AGENT", exitCode: 0 } })).toBe("PAUSE_FOR_AGENT");
  });

  it("actionRequired AGENT_TASK_EXECUTION → PAUSE_FOR_AGENT", () => {
    expect(decide({ result: {
      ok: true, state: "PLAN_READY", exitCode: 0,
      actionRequired: { type: "AGENT_TASK_EXECUTION", taskId: "T-1", changeId: "c1", contextPack: "", allowedFiles: [], expectedNewFiles: [], forbiddenFiles: [], verification: [], resultFile: "", codebase: { provider: "fallback-file-scan", degraded: true } },
    } })).toBe("PAUSE_FOR_AGENT");
  });

  it("BUILD_READY → CONTINUE", () => {
    expect(decide({ result: { ok: true, state: "BUILD_READY", exitCode: 0 } })).toBe("CONTINUE");
  });

  it("ARCHIVED → DONE", () => {
    expect(decide({ result: { ok: true, state: "ARCHIVED", exitCode: 0 } })).toBe("DONE");
  });

  it("E_VERIFY_FAILED → PAUSE_FOR_HUMAN", () => {
    expect(decide({ result: { ok: false, state: "BUILD_READY", exitCode: 7, error: { code: "E_VERIFY_FAILED", message: "" } } })).toBe("PAUSE_FOR_HUMAN");
  });

  it("E_REVIEW_FAILED → PAUSE_FOR_HUMAN", () => {
    expect(decide({ result: { ok: false, state: "VERIFY_READY", exitCode: 8, error: { code: "E_REVIEW_FAILED", message: "" } } })).toBe("PAUSE_FOR_HUMAN");
  });
});
```

- [ ] **Step 3: 运行测试**

```bash
npm test -- packages/core/test/loop-decision.test.ts
```

Expected: 全部 8 个测试通过。

- [ ] **Step 4: 提交**

```bash
git add packages/core/src/loop/loop-decision.ts packages/core/test/loop-decision.test.ts
git commit -m "feat: 新增 LoopDecisionEngine 纯函数，实现 7 种决策规则"
```

---

### Task 3.3: 新增 LoopEventStore

**Files:**
- Create: `packages/core/src/loop/loop-events.ts`
- Create: `packages/core/test/loop-events.test.ts`

- [ ] **Step 1: 创建 loop-events.ts**

```ts
import { appendFile, mkdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import type { Phase } from "../contracts.js";
import type { LoopDecision } from "./model.js";

export type LoopEventType =
  | "LOOP_STARTED"
  | "LOOP_RESUMED"
  | "LOOP_STOPPED"
  | "LOOP_RESTARTED"
  | "COMMAND_STARTED"
  | "COMMAND_FINISHED"
  | "ACTION_REQUIRED"
  | "TASK_COMPLETED"
  | "DECISION_MADE"
  | "STATE_CONVERGED"
  | "LOOP_PAUSED"
  | "LOOP_FAILED"
  | "LOOP_ARCHIVED";

export interface LoopEvent {
  schemaVersion: "1.0.0";
  eventId: string;
  loopId: string;
  runId: string;
  type: LoopEventType;
  phase?: Phase;
  command?: string;
  taskId?: string;
  decision?: LoopDecision;
  data?: Record<string, unknown>;
  createdAt: string;
}

export class LoopEventStore {
  readonly eventsDirectory: string;

  constructor(private readonly root: string) {
    this.eventsDirectory = join(root, ".sdd", "loop", "events");
  }

  async write(runId: string, event: Omit<LoopEvent, "eventId" | "schemaVersion" | "createdAt">): Promise<void> {
    await mkdir(this.eventsDirectory, { recursive: true });
    const fullEvent: LoopEvent = {
      schemaVersion: "1.0.0",
      eventId: randomUUID(),
      createdAt: new Date().toISOString(),
      ...event,
    };
    await appendFile(
      join(this.eventsDirectory, `${runId}.jsonl`),
      `${JSON.stringify(fullEvent)}\n`,
      "utf8",
    );
  }

  async read(runId: string, opts?: { tail?: number }): Promise<LoopEvent[]> {
    try {
      const content = await readFile(
        join(this.eventsDirectory, `${runId}.jsonl`),
        "utf8",
      );
      const lines = content.trim().split("\n").filter(Boolean);
      const events = lines.map((line) => JSON.parse(line) as LoopEvent);
      if (opts?.tail !== undefined && opts.tail > 0) {
        return events.slice(-opts.tail);
      }
      return events;
    } catch {
      return [];
    }
  }
}
```

- [ ] **Step 2: 创建测试**

```ts
// loop-events.test.ts
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { LoopEventStore } from "../src/loop/loop-events.js";

const roots: string[] = [];

afterEach(async () => {
  await Promise.all(roots.splice(0).map((r) => rm(r, { recursive: true })));
});

describe("LoopEventStore", () => {
  it("写入和读取 events JSONL", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-events-"));
    roots.push(root);
    const store = new LoopEventStore(root);
    await store.write("run-1", { loopId: "auto-default", runId: "run-1", type: "LOOP_STARTED" });
    await store.write("run-1", { loopId: "auto-default", runId: "run-1", type: "COMMAND_STARTED", command: "sdd new" });
    const events = await store.read("run-1");
    expect(events).toHaveLength(2);
    expect(events[0]!.type).toBe("LOOP_STARTED");
    expect(events[1]!.type).toBe("COMMAND_STARTED");
  });

  it("--tail 仅返回最后 N 个事件", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-events-"));
    roots.push(root);
    const store = new LoopEventStore(root);
    for (const type of ["a", "b", "c"] as const) {
      await store.write("run-1", { loopId: "auto-default", runId: "run-1", type: "LOOP_STARTED" });
    }
    const events = await store.read("run-1", { tail: 2 });
    expect(events).toHaveLength(2);
  });

  it("不存在的 run 返回空数组", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-events-"));
    roots.push(root);
    const store = new LoopEventStore(root);
    expect(await store.read("nonexistent")).toEqual([]);
  });
});
```

- [ ] **Step 3: 运行测试**

```bash
npm test -- packages/core/test/loop-events.test.ts
```

Expected: 全部通过。

- [ ] **Step 4: 提交**

```bash
git add packages/core/src/loop/loop-events.ts packages/core/test/loop-events.test.ts
git commit -m "feat: 新增 LoopEventStore，支持 JSONL 事件写入和读取（含 --tail）"
```

---

### Task 3.4: 新增 LoopEngine 主类并简化 Core

**Files:**
- Create: `packages/core/src/loop/loop-engine.ts`
- Modify: `packages/core/src/core.ts` (runAuto → 薄封装)
- Modify: `packages/core/src/commands/auto.ts` (prepareAutoLoop 保留为内部方法，由 LoopEngine 调用)

- [ ] **Step 1: 创建 loop-engine.ts**

完整的 LoopEngine 类，包含：
- `run()` 入口（分发 resume/restart/stop/events/status/runAuto）
- `runAuto()` 核心循环（迁移自 Core.runAuto()，增加 DecisionEngine + EventStore）
- `resume()` 恢复逻辑
- `restart()` 重启逻辑
- `stop()` 停止逻辑
- `getEvents()` 读取事件
- `getLoopStatus()` 读取状态

```ts
import { SddError } from "../errors.js";
import type { LoopDecision } from "./model.js";
import type { CommandRequest, CommandResult, CommandName, Phase, AgentActionRequired } from "../contracts.js";
import { COMMANDS } from "../contracts.js";
import { StateStore } from "../state/state-store.js";
import { LoopStore } from "./loop-store.js";
import { LoopEventStore } from "./loop-events.js";
import { decide } from "./loop-decision.js";
import { createDefaultLoopSpec } from "./loop-spec.js";
import { runStatus } from "../commands/status.js";

const COMMAND_BY_PHASE: Partial<Record<Phase, CommandName>> = {
  INDEX_READY: "new",
  SPEC_READY: "design",
  DESIGN_READY: "plan",
  PLAN_READY: "build",
  BUILD_READY: "verify",
  VERIFY_READY: "review",
  REVIEW_READY: "archive",
};

export class LoopEngine {
  constructor(
    private readonly root: string,
    private readonly store: StateStore,
    private readonly loops: LoopStore,
    private readonly events: LoopEventStore,
    private readonly execute: (req: CommandRequest) => Promise<CommandResult>,
  ) {}

  async run(request: CommandRequest): Promise<CommandResult> {
    const args = request.args ?? {};

    if (args.events === true) {
      return this.getEvents(request, typeof args.tail === "number" ? args.tail : undefined);
    }
    if (args.loopStatus === true) {
      return this.getLoopStatus(request);
    }
    if (args.stop === true) {
      return this.stopAuto(request);
    }
    if (args.restart === true) {
      return this.restartAuto(request);
    }
    if (typeof args.resume === "string" || args.resume === true) {
      return this.resumeAuto(request);
    }
    return this.runAuto(request);
  }

  private async runAuto(request: CommandRequest): Promise<CommandResult> {
    let status = await runStatus(this.root);
    if (status.state === "NOT_INITIALIZED") {
      throw new SddError("E_NOT_INITIALIZED", "请先运行 sdd init 再执行 sdd auto", "sdd init");
    }

    const loop = await this.prepareLoop(request.args);
    await this.events.write(loop.runId, {
      loopId: loop.loopId,
      runId: loop.runId,
      type: "LOOP_STARTED",
      phase: status.state,
    });

    for (let step = 0; step < loop.maxSteps; step += 1) {
      if (status.state === "ARCHIVED") {
        await this.finalizeLoop(loop.runId, "ARCHIVED");
        await this.events.write(loop.runId, {
          loopId: loop.loopId, runId: loop.runId,
          type: "LOOP_ARCHIVED", phase: "ARCHIVED",
        });
        return status;
      }

      const command = this.autoCommand(status, request.args);
      if (command === undefined) {
        await this.finalizeLoop(loop.runId,
          status.state === "CLARIFYING" ? "PAUSED" : "RUNNING");
        return status;
      }

      const startedAt = new Date().toISOString();
      const effectiveArgs = command === "build"
        ? { ...request.args, subcommand: "next" }
        : request.args;

      await this.events.write(loop.runId, {
        loopId: loop.loopId, runId: loop.runId,
        type: "COMMAND_STARTED", phase: status.state, command,
      });

      const result = await this.execute({
        command,
        cwd: this.root,
        ...(effectiveArgs === undefined ? {} : { args: effectiveArgs }),
        ...(request.signal === undefined ? {} : { signal: request.signal }),
      });

      await this.events.write(loop.runId, {
        loopId: loop.loopId, runId: loop.runId,
        type: "COMMAND_FINISHED", phase: result.state, command,
      });

      const decision = decide({ result });
      await this.events.write(loop.runId, {
        loopId: loop.loopId, runId: loop.runId,
        type: "DECISION_MADE", decision, phase: result.state,
      });

      await this.recordStep(loop.runId, {
        kind: decision === "PAUSE_FOR_AGENT" ? "AGENT_HANDOFF" : "COMMAND",
        command,
        phaseBefore: status.state,
        phaseAfter: result.state,
        status: !result.ok ? "FAILED"
          : decision === "PAUSE_FOR_AGENT" ? "WAITING_AGENT"
          : decision === "DONE" ? "SUCCEEDED"
          : "SUCCEEDED",
        decision,
        actionRequired: result.actionRequired,
        startedAt,
        endedAt: new Date().toISOString(),
      });

      if (!result.ok || decision === "PAUSE_FOR_AGENT" || decision === "PAUSE_FOR_CLARIFICATION" || decision === "PAUSE_FOR_HUMAN" || decision === "FAIL" || decision === "DONE") {
        const finalStatus = decision === "DONE" ? "ARCHIVED"
          : decision === "PAUSE_FOR_AGENT" || decision === "PAUSE_FOR_CLARIFICATION" || decision === "PAUSE_FOR_HUMAN" ? "PAUSED"
          : "FAILED";

        if (decision === "PAUSE_FOR_AGENT") {
          await this.events.write(loop.runId, {
            loopId: loop.loopId, runId: loop.runId,
            type: "LOOP_PAUSED",
          });
        }

        await this.finalizeLoop(loop.runId, finalStatus);
        return result;
      }

      status = result;
    }

    throw new SddError("E_STATE_CORRUPTED", "auto 流程超过了允许的最大阶段推进次数");
  }

  async resumeAuto(request: CommandRequest): Promise<CommandResult> {
    const args = request.args ?? {};
    const resumeRunId = typeof args.resume === "string" ? args.resume : undefined;
    const state = await this.store.read();

    if (resumeRunId !== undefined) {
      const run = await this.loops.readRun(resumeRunId);
      await this.store.update((current) => ({
        ...current,
        currentRunId: run.runId,
        activeLoop: { loopId: run.loopId, runId: run.runId, status: "RUNNING" },
      }));
    }

    // 如果处于 BUILD_WAITING_AGENT，返回当前 handoff 不推进
    const currentState = await this.store.read();
    if (currentState.currentPhase === "BUILD_WAITING_AGENT") {
      return this.runAuto(request); // runAuto 会检测到 BUILD_WAITING_AGENT 的 build next 逻辑
    }

    return this.runAuto(request);
  }

  async restartAuto(request: CommandRequest): Promise<CommandResult> {
    const state = await this.store.read();
    if (state.activeLoop !== null) {
      const activeLoop = state.activeLoop as { loopId: string; runId: string; status: string };
      const existing = await this.loops.readRun(activeLoop.runId);
      await this.loops.writeRun({
        ...existing,
        status: "ABORTED",
        endedAt: new Date().toISOString(),
      });
      await this.events.write(activeLoop.runId, {
        loopId: activeLoop.loopId, runId: activeLoop.runId,
        type: "LOOP_STOPPED",
      });
    }

    const spec = await this.readLoopSpec();
    const runId = `run-${Date.now()}`;
    const loopId = state.activeLoop !== null && typeof state.activeLoop === "object"
      ? (state.activeLoop as { loopId: string }).loopId
      : spec.loopId;

    await this.loops.writeRun({
      schemaVersion: "1.3.0",
      runId,
      loopId,
      status: "RUNNING",
      startedAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      currentStep: 0,
      steps: [],
    });

    await this.store.update((current) => ({
      ...current,
      currentRunId: runId,
      activeLoop: { loopId, runId, status: "RUNNING" },
    }));

    await this.events.write(runId, {
      loopId, runId, type: "LOOP_RESTARTED",
    });

    return this.runAuto({ ...request, args: { ...request.args, restart: undefined } });
  }

  async stopAuto(request: CommandRequest): Promise<CommandResult> {
    const state = await this.store.read();
    if (state.activeLoop === null || typeof state.activeLoop !== "object") {
      return { ok: false, state: "FAILED", exitCode: 3, error: { code: "E_INVALID_PHASE_COMMAND", message: "没有 active loop" } };
    }

    const activeLoop = state.activeLoop as { loopId: string; runId: string; status: string };
    const run = await this.loops.readRun(activeLoop.runId);
    await this.loops.writeRun({
      ...run,
      status: "ABORTED",
      endedAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    });

    await this.store.update((current) => ({
      ...current,
      activeLoop: { ...activeLoop, status: "ABORTED" },
    }));

    await this.events.write(activeLoop.runId, {
      loopId: activeLoop.loopId, runId: activeLoop.runId,
      type: "LOOP_STOPPED",
    });

    return { ok: true, state: "FAILED", exitCode: 0, data: { loopStopped: activeLoop.runId } };
  }

  async getEvents(request: CommandRequest, tail?: number): Promise<CommandResult> {
    const state = await this.store.read();
    if (state.activeLoop === null || typeof state.activeLoop !== "object") {
      return { ok: true, state: state.currentPhase, exitCode: 0, data: { events: [] } };
    }
    const activeLoop = state.activeLoop as { loopId: string; runId: string; status: string };
    const events = await this.events.read(activeLoop.runId, { tail });
    return { ok: true, state: state.currentPhase, exitCode: 0, data: { events } };
  }

  async getLoopStatus(request: CommandRequest): Promise<CommandResult> {
    const state = await this.store.read();
    if (state.activeLoop === null || typeof state.activeLoop !== "object") {
      return { ok: true, state: state.currentPhase, exitCode: 0, data: { activeLoop: null } };
    }
    const activeLoop = state.activeLoop as Record<string, unknown>;
    return {
      ok: true, state: state.currentPhase, exitCode: 0,
      data: {
        activeLoop: {
          loopId: activeLoop.loopId,
          runId: activeLoop.runId,
          status: activeLoop.status,
          lastDecision: activeLoop.lastDecision,
          waiting: activeLoop.waiting,
          currentPhase: state.currentPhase,
          nextAction: state.suggestedCommand,
        },
      },
    };
  }

  private autoCommand(
    status: CommandResult,
    args: Record<string, unknown> | undefined,
  ): CommandName | undefined {
    if (status.state === "CLARIFYING") {
      return hasAnswers(args) ? "new" : undefined;
    }
    if (status.state === "FAILED" || status.state === "PAUSED") {
      const state = status.data as {
        failedCommand?: string | null;
        interruptedCommand?: string | null;
        suggestedCommand?: string | null;
      } | undefined;
      return parseCommandName(
        state?.interruptedCommand ?? state?.failedCommand ?? state?.suggestedCommand ?? status.next,
      );
    }
    return COMMAND_BY_PHASE[status.state];
  }

  private async prepareLoop(args: Record<string, unknown> | undefined) {
    const state = await this.store.read();
    const spec = await this.readLoopSpec();
    const currentLoop = state.activeLoop !== null && typeof state.activeLoop === "object" && "runId" in state.activeLoop
      ? (state.activeLoop as { loopId: string; runId: string; status: string })
      : null;

    const runId = currentLoop?.runId ?? state.currentRunId ?? `run-${Date.now()}`;
    const loopId = currentLoop?.loopId ?? spec.loopId;

    if (!(await this.loops.hasRun(runId))) {
      await this.loops.writeRun({
        schemaVersion: "1.3.0",
        runId,
        loopId,
        status: "RUNNING",
        startedAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        currentStep: 0,
        steps: [],
      });
    }

    if (currentLoop === null) {
      await this.store.update((current) => ({
        ...current,
        currentRunId: runId,
        activeLoop: { loopId, runId, status: "RUNNING" },
      }));
    }

    return { loopId, runId, maxSteps: spec.maxSteps };
  }

  private async recordStep(runId: string, step: {
    kind: "COMMAND" | "AGENT_HANDOFF";
    command: string;
    phaseBefore: string;
    phaseAfter?: string;
    status: "SUCCEEDED" | "FAILED" | "WAITING_AGENT";
    decision?: LoopDecision;
    actionRequired?: AgentActionRequired;
    startedAt: string;
    endedAt: string;
  }) {
    const run = await this.loops.readRun(runId);
    await this.loops.writeRun({
      ...run,
      updatedAt: new Date().toISOString(),
      currentStep: run.steps.length + 1,
      lastDecision: step.decision,
      steps: [...run.steps, {
        step: run.steps.length + 1,
        kind: step.kind,
        command: step.command,
        phaseBefore: step.phaseBefore,
        phaseAfter: step.phaseAfter,
        status: step.status,
        decision: step.decision,
        actionRequired: step.actionRequired,
        startedAt: step.startedAt,
        endedAt: step.endedAt,
      }],
      status: step.status === "FAILED" ? "FAILED"
        : step.status === "WAITING_AGENT" ? "WAITING_AGENT"
        : "RUNNING",
    });
  }

  private async finalizeLoop(runId: string, status: "RUNNING" | "PAUSED" | "FAILED" | "ARCHIVED") {
    const run = await this.loops.readRun(runId);
    await this.loops.writeRun({
      ...run,
      status,
      updatedAt: new Date().toISOString(),
      ...(status === "ARCHIVED" || status === "FAILED" || status === "PAUSED"
        ? { endedAt: new Date().toISOString() }
        : {}),
    });
    await this.store.update((current) => ({
      ...current,
      activeLoop: current.activeLoop === null || typeof current.activeLoop !== "object"
        ? current.activeLoop
        : { ...(current.activeLoop as Record<string, unknown>), runId, status: status === "ARCHIVED" ? "SUCCEEDED" : status },
    }));
  }

  private async readLoopSpec() {
    try { return await this.loops.readSpec(); }
    catch { return createDefaultLoopSpec(); }
  }
}

function hasAnswers(args: Record<string, unknown> | undefined): boolean {
  const answers = args?.answers;
  return answers !== undefined && typeof answers === "object" && answers !== null && Object.keys(answers).length > 0;
}

function parseCommandName(input: string | undefined | null): CommandName | undefined {
  if (input === undefined || input === null) return undefined;
  const normalized = input.replace(/^\/?sdd[.\s]/, "") as CommandName;
  return (COMMANDS as readonly string[]).includes(normalized) ? normalized : undefined;
}
```

- [ ] **Step 2: 简化 Core.runAuto() 为薄封装**

将 `packages/core/src/core.ts` 中的 `private async runAuto()` 方法从 ~90 行简化为：

```ts
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

import 需要新增：
```ts
import { LoopEngine } from "./loop/loop-engine.js";
import { LoopEventStore } from "./loop/loop-events.js";
```

删除不再直接使用的 import：
- `prepareAutoLoop`, `recordAutoStep`, `finalizeAutoLoop`（从 `./commands/auto.js` 的 import）
- `autoCommand` 函数（Core 内的 private 函数）
- `hasAnswers`, `parseCommandName`（Core 内的 private 函数）

- [ ] **Step 3: 运行类型检查和测试**

```bash
npm run typecheck && npm test
```

需要修复：
- 现有 `auto.ts` 的 prepareAutoLoop/recordAutoStep/finalizeAutoLoop 如果还有外部引用，确认兼容
- 确保 `auto.test.ts` 中的测试仍然通过（prepareAutoLoop 等的调用链变化）

- [ ] **Step 4: 提交**

```bash
git add packages/core/src/loop/loop-engine.ts packages/core/src/core.ts packages/core/src/commands/auto.ts packages/core/test/
git commit -m "feat: 新增 LoopEngine，Core.runAuto() 改为薄封装"
```

---

## Milestone 4：Auto 命令增强

### Task 4.1: CLI 新 flags 解析和路由

**Files:**
- Modify: `packages/cli/src/cli.ts`

- [ ] **Step 1: 新增 parseArgs options**

在 `packages/cli/src/cli.ts` 的 `parseArgs` options 中新增：

```ts
resume:     { type: "boolean", default: false },
restart:    { type: "boolean", default: false },
stop:       { type: "boolean", default: false },
events:     { type: "boolean", default: false },
tail:       { type: "string" },
loop:       { type: "boolean", default: false },
"loop-status": { type: "boolean", default: false },
run:        { type: "string" },
```

- [ ] **Step 2: 修改 case "auto" 分支**

```ts
case "auto": {
  if (values.resume) extraArgs.resume = values.run ?? true;
  if (values.restart) extraArgs.restart = true;
  if (values.stop) extraArgs.stop = true;
  if (values.events) { extraArgs.events = true; if (values.tail) extraArgs.tail = Number(values.tail); }
  if (values["loop-status"]) extraArgs.loopStatus = true;
  const requirement = positionals.slice(1).join(" ") || "";
  result = await runAuto(core, cwd, requirement, extraArgs, undefined);
  break;
}
```

- [ ] **Step 3: 修改 case "status" 分支**

```ts
case "status":
  extraArgs.loop = values.loop;
  result = await runStatus(core, cwd, extraArgs, undefined);
  break;
```

- [ ] **Step 4: 更新 HELP_TEXT**

```ts
const HELP_TEXT = `sdd — SDD Agent Harness CLI

用法: sdd <command> [options]

命令:
  init              初始化 .sdd/
  status            显示当前 SDD 状态
  new <需求>         创建新变更
  design            生成设计制品
  plan              生成实施计划
  build             构建 (build next / build complete)
  verify            验证
  review            审查
  archive           归档
  auto <需求>        自动推进 SDD Loop
  auto --resume     恢复当前 auto run
  auto --restart    重启 auto run
  auto --stop       停止当前 auto run
  auto --events     查看 auto run 事件
  auto --status     查看 auto run 状态
  codebase           代码库上下文管理 (status/doctor/index/query/rebuild)

通用参数:
  --json            JSON 输出
  --cwd <path>      项目根目录 (默认当前目录)
  --change <id>     指定变更 ID
  --timeout <s>     超时秒数
  --non-interactive 禁止交互
  --force           强制
  --verbose         详细输出
  --help            帮助
  --version         版本
`;
```

- [ ] **Step 5: 运行类型检查**

```bash
npm run typecheck
```

- [ ] **Step 6: 提交**

```bash
git add packages/cli/src/cli.ts
git commit -m "feat: CLI 新增 --resume/--restart/--stop/--events/--status/--loop flags 和 help 更新"
```

---

### Task 4.2: 文本输出显示 actionRequired

**Files:**
- Modify: `packages/cli/src/json-output.ts`

- [ ] **Step 1: 修改 outputText()**

在 `packages/cli/src/json-output.ts` 的 `outputText()` 函数中，在现有 Warning/Error 输出之后，增加 actionRequired 输出：

```ts
if (result.actionRequired) {
  const ar = result.actionRequired;
  console.log(`\nAction Required: ${ar.type}`);
  console.log(`Task: ${ar.taskId}`);
  console.log(`Context Pack: ${ar.contextPack}`);
  console.log(`Result File: ${ar.resultFile}`);
  if (ar.allowedFiles.length > 0) {
    console.log(`\nAllowed Files:`);
    for (const f of ar.allowedFiles) console.log(`  - ${f}`);
  }
  if (ar.expectedNewFiles.length > 0) {
    console.log(`\nExpected New Files:`);
    for (const f of ar.expectedNewFiles) console.log(`  - ${f}`);
  }
  if (ar.forbiddenFiles.length > 0) {
    console.log(`\nForbidden Files:`);
    for (const f of ar.forbiddenFiles) console.log(`  - ${f}`);
  }
  if (ar.verification.length > 0) {
    console.log(`\nVerification:`);
    for (const v of ar.verification) console.log(`  - ${v.command} ${v.args.join(" ")}`);
  }
}
```

- [ ] **Step 2: 更新 CLI golden 测试**

在 `packages/cli/test/cli.test.ts` 中增加文本输出验证（如果已有 snapshot 测试则更新 snapshot）。

- [ ] **Step 3: 运行测试并提交**

```bash
npm test
git add packages/cli/src/json-output.ts packages/cli/test/cli.test.ts
git commit -m "feat: CLI 文本输出显示 actionRequired 详细信息"
```

---

### Task 4.3: status --loop 支持

**Files:**
- Modify: `packages/core/src/commands/status.ts`
- Modify: `packages/cli/src/commands/status.ts`

- [ ] **Step 1: Core runStatus 支持 --loop**

在 `packages/core/src/commands/status.ts` 的 `runStatus()` 函数中，在返回结果中增加 `activeLoop` 信息：

在现有 `return` 之前：
```ts
let loopData: unknown = undefined;
if (args.loop === true) {
  const activeLoop = state.activeLoop;
  if (activeLoop !== null && typeof activeLoop === "object") {
    const loop = activeLoop as Record<string, unknown>;
    loopData = {
      loopId: loop.loopId,
      runId: loop.runId,
      status: loop.status,
      waiting: loop.waiting,
    };
  }
}
```

在返回对象中增加：
```ts
data: { ...state, activeLoop: loopData },
```

函数签名需要改为接收 args：
```ts
export async function runStatus(root: string, args?: Record<string, unknown>): Promise<CommandResult>
```

同步更新调用方 `core.ts` 中的 `runStatus(request.cwd)` → `runStatus(request.cwd, request.args)`。

- [ ] **Step 2: 运行测试并提交**

```bash
npm test
git add packages/core/src/commands/status.ts packages/core/src/core.ts packages/core/src/loop/loop-engine.ts packages/core/src/commands/recovery.ts
git commit -m "feat: status --loop 返回 activeLoop 摘要信息"
```

---

## Milestone 5：Result v2 与集成测试

### Task 5.1: build complete 支持 v2 result

**Files:**
- Modify: `packages/core/src/commands/build.ts` (buildCompleteTask)

- [ ] **Step 1: 在 buildCompleteTask 中增加 v2 result 判断**

在 `resultJson` 校验后、现有 v1 逻辑之前，增加：

```ts
// v2 result 处理：检查是否是 TaskExecutionResultV2
if (resultJson.schemaVersion === "1.2.0" && resultJson.fileDelta) {
  // v2 result 必须包含 legacy
  if (!resultJson.legacy) {
    return {
      ok: false,
      state: "FAILED",
      exitCode: 7,
      error: {
        code: "E_TDD_EVIDENCE_REQUIRED",
        message: "v2 task execution result 必须包含 legacy TDD evidence",
      },
    };
  }
  // 使用 legacy 继续现有流程（path-through）
}
```

- [ ] **Step 2: 编写 v2 result 测试**

```ts
it("build complete 接受含 legacy 的 v2 result", async () => {
  // v2 result 包含 legacy，正常 complete
});

it("build complete 拒绝不含 legacy 的 v2 result", async () => {
  // v2 result 不含 legacy，返回 E_TDD_EVIDENCE_REQUIRED
});
```

- [ ] **Step 3: 运行测试并提交**

```bash
npm test
git add packages/core/src/commands/build.ts packages/core/test/build.test.ts
git commit -m "feat: build complete 支持 v2 result（要求必须含 legacy evidence）"
```

---

### Task 5.2: 集成测试补齐

**Files:**
- Modify: `packages/core/test/auto.test.ts`
- Modify: `packages/core/test/state.test.ts`
- Create: `packages/core/test/integration.test.ts` (可选)

- [ ] **Step 1: 端到端测试**

```ts
it("init → auto → build next → build complete → auto --resume → archive 完整链路", async () => {
  // 完整 SDD 流程
});
```

- [ ] **Step 2: auto --restart/--stop/--events/--status 行为测试**

```ts
it("auto --restart abort 旧 run 并创建新 run", async () => {});
it("auto --stop 不破坏 change", async () => {});
it("auto --events 返回事件列表", async () => {});
it("auto --status 返回 loop 状态", async () => {});
```

- [ ] **Step 3: Schema 迁移幂等测试**

```ts
it("1.2.0→1.3.0 迁移可重复执行不损坏 state", async () => {});
```

- [ ] **Step 4: 运行全部测试并提交**

```bash
npm test
git add packages/core/test/
git commit -m "test: 补齐端到端、迁移和 auto 子命令的集成测试"
```

---

### Task 5.3: 最终验证

- [ ] **Step 1: 运行全量检查**

```bash
npm run format:check && npm run lint && npm run typecheck && npm test
```

Expected: 所有检查通过，所有测试通过。

- [ ] **Step 2: 验证 AC 覆盖**

对照 `docs/四期需求文档.md` 第 21 节验收标准（AC-001 ~ AC-034），逐一确认有对应测试覆盖。

- [ ] **Step 3: 提交最终版本**

```bash
git add -A && git commit -m "feat: Auto Loop Engine 正式化完成，全部 34 项 AC 覆盖"
```
