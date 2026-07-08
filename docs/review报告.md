# sdd-harness 三期全局 Review 报告

## 评审范围

仓库：`liuyi-it/sdd-harness`

本报告基于 GitHub 当前最新代码进行全局 Review，不只看本次提交，而是按三期整体目标审查：

- CLI-first
- Agent-agnostic
- 支持 Claude Code、Codex、OpenCode、Kimi Code、GitHub Copilot CLI、自研 Coding Agent
- Node.js >= 22
- 内置 `codebase-memory-mcp`
- MCP 不可用时自动降级为 `fallback-file-scan`
- 降级时必须明确提示用户诊断
- `sdd build next / complete` 支撑 Agent 执行闭环
- 不发布 npm / GitHub Packages
- 不提交 `package-lock.json`

---

# 一、总体结论

当前版本相比上一轮已经有明显修复：

1. CLI 已经把 `--change` 映射为 `changeId`。
2. CLI 已经把 `--non-interactive` 映射为 `nonInteractive`。
3. CLI 已经注入 `CodebaseMemoryManager → CodebaseMemoryTransport → CodebaseAdapter → Core`。
4. Core 命令契约已经加入 `codebase`。
5. Core 已经增加 `codebase` 命令分发入口。
6. `sdd new` 已经支持未传 `changeId` 时自动生成 `change-${Date.now()}`。
7. Agent Protocol 已经从 `1.0.0` 改到 `1.2.0`。

但当前代码仍然没有达到三期验收标准，仍存在 P0 阻断项。

核心问题集中在：

```text
1. codebase-memory-mcp 生命周期仍未真正跑通
2. sdd codebase index/rebuild 未实现
3. sdd codebase query 参数没有正确传递
4. sdd auto 到 build 阶段仍会失败
5. build next / complete 只是雏形，尚未形成 verify/review/archive 可用闭环
6. build complete 没有写 task-results.json，也没有持久化 BUILD_READY
7. Agent result 校验、安全校验、Git delta 校验仍不完整
```

当前不建议发布，也不建议交给真实 Agent 正式使用。

---

# 二、P0 阻断问题

## P0-1：`codebase-memory-mcp` 仍然不会被正常启动，`sdd init` 大概率永远 fallback

### 问题说明

`CodebaseAdapter.initialize()` 当前逻辑是先判断：

```ts
if (await this.transport.isAvailable()) {
  await this.transport.index(root);
  ...
}
```

只有 `isAvailable()` 返回 true 才会调用 `index(root)`。

但 `CodebaseMemoryTransport.isAvailable()` 当前逻辑是：

```ts
if (!this.initialized) return false;
```

而 `initialized = true` 是在 `index(root)` 中设置的。

因此形成死锁：

```text
CodebaseAdapter.initialize()
→ transport.isAvailable()
→ initialized=false
→ 返回 false
→ 不调用 transport.index()
→ CodebaseMemoryManager.initialize() 永远不会被触发
→ MCP 永远不会启动
→ 永远 fallback
```

README 声称 `sdd init` 会自动通过 `npx` 启动托管的 `codebase-memory-mcp`，MCP 不可用时才降级。

当前代码不符合该行为。

### 修复要求

修改 `CodebaseAdapter.initialize()` 启动顺序：

```ts
if (this.transport !== undefined) {
  try {
    await this.transport.index(root);

    if (await this.transport.isAvailable()) {
      return {
        provider: "codebase-memory-mcp",
        degraded: false,
        ...(await this.transport.summarize(root)),
        diagnostics: ...
      };
    }
  } catch (error) {
    return fallback(...);
  }
}

return fallback(...);
```

或者修改 `CodebaseMemoryTransport.isAvailable()`，使其在未初始化时能进行轻量探测，但更推荐由 `initialize()` 明确驱动生命周期。

### 验收标准

执行：

```bash
sdd init --json
```

当 MCP 可用时：

```json
{
  "ok": true,
  "state": "INDEX_READY",
  "data": {
    "provider": "codebase-memory-mcp",
    "degraded": false
  }
}
```

当 MCP 不可用时：

```json
{
  "warnings": [
    {
      "code": "W_CODEBASE_MEMORY_UNAVAILABLE",
      "next": "sdd codebase doctor"
    }
  ]
}
```

并且 `.sdd/index/codebase-diagnostics.json` 必须存在。

---

## P0-2：MCP diagnostics 存在假阳性

### 问题说明

`CodebaseMemoryTransport.inspect()` 根据 `manager.getCapabilities()` 判断 degraded。

但 `CodebaseMemoryManager.getCapabilities()` 只看 config mode，不看 MCP 是否真的启动、握手、可调用或索引完成。只要 mode 不是 fallback，就返回：

```ts
provider: "codebase-memory-mcp";
```

这会导致 diagnostics 中可能出现：

```json
{
  "installed": true,
  "configured": true,
  "connected": true,
  "callable": true,
  "indexed": true
}
```

但实际 MCP 并没有启动成功。

### 修复要求

`CodebaseMemoryManager` 必须维护真实状态：

```ts
private lifecycleResult: McpLifecycleResult | null;
private lastDiagnostics: McpDiagnostics | null;
private initializedRoot: string | null;
```

`getCapabilities()` 和 `inspect()` 必须基于真实状态：

- MCP 进程是否启动
- MCP protocol 是否握手成功
- tools/list 是否成功
- index 是否完成
- query 是否可调用
- fallback 是否启用

### 验收标准

模拟 MCP 不可用时：

```bash
sdd codebase doctor --json
```

必须显示：

```json
{
  "data": {
    "provider": "fallback-file-scan",
    "degraded": true
  },
  "warnings": [
    {
      "code": "W_CODEBASE_MEMORY_UNAVAILABLE",
      "next": "sdd codebase doctor"
    }
  ]
}
```

不得显示 `connected/callable/indexed=true`。

---

## P0-3：`sdd codebase index` 和 `sdd codebase rebuild` 仍未实现

### 问题说明

CLI 允许：

```ts
const validSubcommands = ["status", "doctor", "index", "query", "rebuild"];
```

但 Core 的 `runCodebaseCommand()` 只实现了：

```text
status
doctor
query
```

`index` 和 `rebuild` 会走 default，返回“暂未实现”。

README 已经把 `index` 和 `rebuild` 列为正式命令。

### 修复要求

补齐：

```bash
sdd codebase index
sdd codebase rebuild
```

行为要求：

#### `sdd codebase index`

- 主动触发 MCP index。
- MCP 不可用时触发 fallback-file-scan。
- 写入：
  - `.sdd/index/codebase-summary.md`
  - `.sdd/index/package-structure.md`
  - `.sdd/index/architecture.md`
  - `.sdd/index/codebase-diagnostics.json`
  - `.sdd/index/mcp-capabilities.json`

#### `sdd codebase rebuild`

- 清理旧索引。
- 重新执行 index。
- 不破坏 `.sdd/state.json` 中当前 change 状态。
- 返回 provider、degraded、diagnostics、warnings。

### 验收标准

```bash
sdd codebase index --json
sdd codebase rebuild --json
```

必须返回 `ok: true`，并且生成或刷新 `.sdd/index/*` 制品。

---

## P0-4：`sdd codebase query <q>` 没有把 `<q>` 正确传入

### 问题说明

CLI 当前只读取第一个 positional 作为 subcommand：

```ts
const codebasePositionals = positionals.slice(1);
const subcommand = codebasePositionals[0];
result = await runCodebase(core, cwd, subcommand, extraArgs, undefined);
```

没有把后续参数拼成 query。

Core 里 query 从 `args.query` 读取：

```ts
const query = (args?.query as string) || "";
```

但 CLI 没有设置 `extraArgs.query`。

此外，`CodebaseMemoryTransport.query()` 当前传给 manager 的是：

```ts
query: input.requirement ?? input.intent;
```

它没有使用 `input.query`。

### 修复要求

CLI 修改为：

```ts
case "codebase": {
  const [subcommand, ...queryParts] = positionals.slice(1);
  const query = queryParts.join(" ");
  if (query) extraArgs.query = query;
  if (values.intent) extraArgs.intent = values.intent;
  result = await runCodebase(core, cwd, subcommand, extraArgs, undefined);
}
```

Transport 修改为：

```ts
query: input.query ?? input.requirement ?? input.intent;
```

同时建议 `CodebaseAdapter.query()` 不要硬编码 root 为 `"."`。当前调用为：

```ts
const raw = await this.transport.query(".", input);
```

应改成显式传 root：

```ts
async query(root: string, input: McpQueryInput)
```

### 验收标准

```bash
sdd codebase query "OrderService" --json
```

结果中的 query input 必须包含 `"OrderService"`，不得退化为 `"impact"` 或空字符串。

---

## P0-5：`sdd auto` 到 build 阶段仍会失败

### 问题说明

README 快速开始推荐：

```bash
sdd init
sdd auto "实现订单取消功能"
```

Core 的 auto 映射是：

```ts
PLAN_READY: "build";
```

这会调用无子命令的完整 build 流程。完整 build 需要 `TaskExecutor`。

但 CLI 默认构造 Core 时只注入了 codebase，没有注入 taskExecutor。

Core 默认 `taskExecutor` 是 `MissingTaskExecutor`。

`MissingTaskExecutor` 会直接抛错。

### 修复要求

`auto` 到 `PLAN_READY` 时，不应直接调用传统 `build`，而应返回 `build next` 的 `actionRequired`。

建议在 `runAuto()` 中特殊处理：

```ts
if (status.state === "PLAN_READY") {
  return this.execute({
    command: "build",
    cwd: request.cwd,
    args: {
      ...request.args,
      subcommand: "next",
    },
    signal: request.signal,
  });
}
```

### 验收标准

```bash
sdd auto "实现订单取消功能" --json
```

当流程推进到 build 阶段时，应返回：

```json
{
  "ok": true,
  "state": "BUILDING",
  "actionRequired": {
    "type": "AGENT_TASK_EXECUTION"
  }
}
```

不得因为 `MissingTaskExecutor` 失败。

---

## P0-6：`build complete` 不写 `task-results.json`，导致后续 `verify` 失败

### 问题说明

`buildCompleteTask()` 当前只更新 `.sdd/state.json.tasks[taskId]`：

```ts
tasks: { ...current.tasks, [taskId]: status as "DONE" | "FAILED" }
```

但没有写入：

```text
.sdd/changes/<changeId>/task-results.json
```

`verify` 阶段明确读取：

```ts
readFile(join(change, "task-results.json"), "utf8");
```

传统完整 build 流程会写 `task-results.json`。

### 修复要求

`buildCompleteTask()` 必须：

1. 读取已有 `task-results.json`，不存在则使用 `[]`。
2. 将 Agent result 转成 Core `TaskResult`。
3. 替换同 taskId 的旧结果。
4. 写回 `.sdd/changes/<changeId>/task-results.json`。
5. 写入 `.sdd/runs/<runId>/tasks/<taskId>.result.json`。
6. 写入 audit log。

### 验收标准

执行：

```bash
sdd build complete --task T001 --result .sdd/runs/<runId>/tasks/T001.result.json --json
```

必须生成或更新：

```text
.sdd/changes/<changeId>/task-results.json
```

随后：

```bash
sdd verify --json
```

不得因为缺少 `task-results.json` 失败。

---

## P0-7：`build complete` 返回 `BUILD_READY`，但没有持久化 `BUILD_READY`

### 问题说明

`buildCompleteTask()` 最后返回：

```ts
state: allDone ? "BUILD_READY" : "BUILDING";
```

但它没有把 `.sdd/state.json.currentPhase` 更新为 `BUILD_READY`。

`verify` 要求持久状态必须是 `BUILD_READY` 或 `VERIFY_READY`。

### 影响

CLI 返回看似成功，但下一步 `sdd verify` 会读取旧状态 `BUILDING`，从而拒绝执行。

### 修复要求

当 `allDone === true` 时，必须持久化：

```ts
currentPhase: "BUILD_READY",
inProgressPhase: null,
failedCommand: null,
failedReason: null,
interruptedCommand: null,
lastCommand: "sdd build complete",
lastError: null,
suggestedCommand: "sdd verify"
```

当未全部完成时，应持久化：

```ts
currentPhase: "BUILDING",
inProgressPhase: "BUILDING",
lastCommand: "sdd build complete",
suggestedCommand: "sdd build next"
```

### 验收标准

所有任务完成后：

```bash
sdd status --json
```

必须显示：

```json
{
  "state": "BUILD_READY",
  "next": "sdd verify"
}
```

---

# 三、P1 高风险问题

## P1-1：`build next` 会覆盖 plan 阶段生成的 Context Pack

### 问题说明

`plan` 阶段已经生成完整 context pack，包括 rules、spec/design/impact、tasksJson、metadata 等。

但 `buildNextTask()` 会直接覆盖同一路径：

```text
.sdd/context-packs/<changeId>/<taskId>.md
```

只写入极简内容。

### 修复要求

`build next` 不应重写 context pack。它应该只读取 plan 生成的 context pack：

```ts
const contextPackPath = `.sdd/context-packs/${changeId}/${nextTask.id}.md`;
await access(join(root, contextPackPath));
```

如果缺失，应返回 `E_MISSING_ARTIFACT`。

---

## P1-2：`build next` 不把任务标记为 BUILDING，重复调用会重复领取同一任务

### 问题说明

`buildNextTask()` 只排除 `DONE` 任务。

但返回任务后没有设置：

```ts
tasks[nextTask.id] = "BUILDING";
```

只更新了阶段和 suggestedCommand。

### 修复要求

`build next` 返回任务时应更新：

```ts
tasks: {
  ...current.tasks,
  [nextTask.id]: "BUILDING"
}
```

并且下次选择任务时应排除 `BUILDING`，除非显式指定重取当前任务。

---

## P1-3：`build next / complete` 没有 FileLock，存在并发风险

### 问题说明

传统 build 有 FileLock。

但 `buildNextTask()` 和 `buildCompleteTask()` 没有加锁。

### 修复要求

`build next` 和 `build complete` 都必须加锁：

```ts
const lock = new FileLock(root);
await lock.acquire("sdd build", undefined, lockOptions(rawArgs));
try {
  ...
} finally {
  await lock.release();
}
```

---

## P1-4：`build complete` 文件范围校验过弱

### 问题说明

当前只检查：

```ts
const modifiedFiles = (resultJson.modifiedFiles as string[]) ?? [];
const allowedFiles = task.allowedFiles ?? [];
const outOfScope = modifiedFiles.filter((f) => !allowedFiles.includes(f));
```

问题：

- 不支持 glob/pattern。
- 不校验 expectedNewFiles。
- 不校验 deleted files。
- 不读取真实 Git delta。
- Agent 可以漏报 modifiedFiles 绕过检查。

### 修复要求

复用传统 build 的安全校验：

```ts
validateTaskFiles(actualFiles, task);
```

并基于真实 Git delta，而不是只相信 Agent result。

---

## P1-5：`build complete` 没复用 `validateTaskResult()` 或 Core normalizer

### 问题说明

Agent Protocol validator 已经支持 `schemaVersion: "1.2.0"`。

但 `buildCompleteTask()` 只做浅校验：

```ts
!resultJson.schemaVersion ||
!resultJson.taskId ||
!VALID_TASK_STATUSES.includes(...)
```

同时 Core 内部 `TaskExecutionResultV2` 又是另一套结构。

### 修复要求

建议采用：

```text
外部 Agent Result: packages/agent-protocol
内部 Core Result: TaskExecutionResult / TaskExecutionResultV2
```

`build complete` 流程：

```text
读取 result json
→ validateTaskResult()
→ 转换为 Core TaskResult
→ validateExecution()
→ 写 task-results.json
→ 写 run result artifact
→ 更新 state
```

---

## P1-6：`build complete` 没完整校验 TDD / verification evidence

### 问题说明

当前 TDD 校验逻辑存在问题：

```ts
if (missingPhases.length > 0 && tddEvidence && tddEvidence.length > 0)
```

这意味着：

- `tddEvidence` 为空数组时不会报错。
- verification 失败不会阻断。
- command 白名单没有校验。
- verification command 是否符合任务要求没有校验。

### 修复要求

必须复用或抽取传统 build 中的校验逻辑：

- `taskEvidenceFailures()`
- `tddChainFailures()`
- `isCommandAllowed()`
- `validateTaskFiles()`

---

# 四、P2 设计一致性问题

## P2-1：`codebase status` 有副作用

当前 `status` 调用了 `codebase.initialize(root)`。

这可能启动 MCP、扫描文件、写 diagnostics。是否接受取决于设计。

建议：

```text
status：只读读取已有 diagnostics
doctor：主动诊断
index：主动索引
rebuild：清理并重建
```

---

## P2-2：MCP 官方 URL 不一致

Core 中 URL 是：

```text
https://github.com/DeusData/codebase-memory-mcp
```

Transport 中 URL 是：

```text
https://github.com/liuyi-it/codebase-memory-mcp
```

需要统一。

---

## P2-3：README 中 `sdd auto` 仍然过度承诺

README 当前仍把 `sdd auto "..."` 作为快速开始主流程。

在 P0-5 修复前，建议 README 改为阶段式流程，或者明确说明 `auto` 会在 build 阶段返回 `actionRequired`。

---

# 五、建议修复顺序

## 第一轮：修主链路

1. 修 `CodebaseAdapter.initialize()` 与 `CodebaseMemoryTransport.isAvailable()` 的启动死锁。
2. 修 `sdd codebase query` 参数传递。
3. 实现 `sdd codebase index`。
4. 实现 `sdd codebase rebuild`。
5. 修 `sdd auto` 到 `PLAN_READY` 后返回 `build next` 的 `actionRequired`。
6. 修 `build complete` 写 `task-results.json`。
7. 修 `build complete` 所有任务完成后持久化 `BUILD_READY`。

## 第二轮：修安全和一致性

8. `build next / complete` 加 FileLock。
9. `build next` 不覆盖 context pack。
10. `build next` 把任务标记为 `BUILDING`。
11. `build complete` 使用真实 Git delta 校验文件范围。
12. `build complete` 复用 `validateTaskResult()` 和 Core evidence 校验。
13. 统一 MCP diagnostics 真实状态。

## 第三轮：补测试和文档

14. 补 E2E 测试：
    - `init → new → design → plan`
    - `build next → build complete → verify`
    - `auto → actionRequired`
    - `codebase query`
    - MCP unavailable fallback

15. 更新 README，避免 `sdd auto` 在未完全修通前过度承诺。
16. 保持不发布 npm / GitHub Packages。
17. 保持不提交 `package-lock.json`。

---

# 六、当前最终判断

当前代码已经解决了上一轮的一部分结构性问题，但还没有满足三期可验收标准。

不能通过的关键验收：

```bash
sdd init
sdd auto "xxx"
```

当前仍会在 build 阶段失败。

```bash
sdd codebase index
sdd codebase rebuild
```

当前仍未实现。

```bash
sdd build next
sdd build complete
sdd verify
```

当前不能可靠闭环，因为 `build complete` 没写 `task-results.json`，也没持久化 `BUILD_READY`。

建议先修完 P0-1 到 P0-7，再进入下一轮全链路验收。
