# sdd-harness 三期代码 Review 修复报告

## 背景

当前仓库：`liuyi-it/sdd-harness`

三期目标是将项目升级为 CLI-first、Agent-agnostic、codebase-memory-powered、verification-gated 的 SDD Harness，支持：

- Claude Code
- Codex
- OpenCode
- Kimi Code
- GitHub Copilot CLI
- 自研 Coding Agent
- Node.js >= 22
- 内置 `codebase-memory-mcp`
- MCP 不可用时自动降级为 `fallback-file-scan`，且必须明确提示用户诊断
- CLI 作为唯一确定性入口

当前主架构方向基本正确，但主链路尚未闭合，存在多个 P0 阻断问题。请按本文优先级修复。

---

# P0 阻断问题

## P0-1：CLI 无法正常创建新变更，`sdd new` / `sdd auto` 主流程会失败

### 现象

README 的快速开始是：

```bash
sdd init
sdd auto "实现订单取消功能"
```

README 明确把 `sdd auto "..."` 作为主入口使用方式。

但当前 `runNew()` 在非继续执行场景中要求 `args.changeId` 必须存在，否则抛出：

```ts
throw new SddError("E_MISSING_CHANGE", "缺少必需的变更 id");
```

相关代码位于 `packages/core/src/commands/new.ts`。

同时 CLI 解析 `--change` 后传给 Core 的字段是 `change`，不是 `changeId`：

```ts
if (values.change) extraArgs.change = values.change;
```

相关代码位于 `packages/cli/src/cli.ts`。

但 Core 内部统一读取的是 `args.changeId`。

### 影响

以下命令会失败或行为不符合预期：

```bash
sdd new "xxx"
sdd new "xxx" --change abc
sdd auto "xxx"
```

### 修复要求

1. CLI 层把 `--change` 映射为 `changeId`：

```ts
if (values.change) extraArgs.changeId = values.change;
```

2. `runNew()` 支持未传 `changeId` 时自动生成变更 ID，例如：

```ts
const changeId = continuing
  ? state.currentChangeId
  : (args.changeId ?? `change-${Date.now()}`);
```

3. 自动生成的 changeId 应满足：
   - 文件路径安全
   - 可读
   - 稳定写入 `.sdd/state.json`
   - 后续 `design/plan/build/verify/review/archive` 能通过 `currentChangeId` 继续执行

### 验收用例

```bash
sdd init
sdd new "实现订单取消功能"
sdd status
```

期望：

- `sdd new` 成功进入 `SPEC_READY` 或 `CLARIFYING`
- `.sdd/state.json.currentChangeId` 非空
- `.sdd/changes/<changeId>/` 已生成

```bash
sdd init
sdd auto "实现订单取消功能"
```

期望：

- 不因为缺少 changeId 失败
- 能至少推进到 `CLARIFYING`、`SPEC_READY` 或后续阶段

---

## P0-2：`sdd codebase doctor/index/query/rebuild` 实际没有执行

### 现象

CLI 的 `runCodebase()` 校验子命令后，把所有 codebase 子命令都包装成：

```ts
command: "status"
args: { ...args, codebaseSubcommand: subcommand }
```

相关代码位于 `packages/cli/src/commands/codebase.ts`。

但 `Core.execute()` 对 `status` 是直接短路：

```ts
if (request.command === "status")
  return withVerboseData(await runStatus(request.cwd), request);
```

它完全不读取 `codebaseSubcommand`。

### 影响

以下命令目前大概率只是普通 `sdd status`：

```bash
sdd codebase status
sdd codebase doctor
sdd codebase index
sdd codebase query "xxx"
sdd codebase rebuild
```

README 明确将这些命令列为正式功能。

### 修复要求

1. 把 `codebase` 加入 Core 命令契约：

```ts
export const COMMANDS = [
  "init",
  "auto",
  "new",
  "design",
  "plan",
  "build",
  "verify",
  "review",
  "archive",
  "status",
  "codebase",
] as const;
```

当前 `COMMANDS` 不包含 `codebase`。

2. `packages/cli/src/commands/codebase.ts` 应传：

```ts
command: "codebase"
args: { ...args, subcommand, query }
```

而不是伪装成 `status`。

3. 在 Core 中新增 `codebase` 分发逻辑，例如：

```ts
if (request.command === "codebase") {
  return runCodebaseCommand(
    request.cwd,
    this.codebase,
    request.args,
    request.signal,
  );
}
```

4. 实现至少以下子命令：
   - `status`：显示 provider、degraded、diagnostics 路径
   - `doctor`：执行 MCP 可用性诊断，输出明确修复建议
   - `index`：触发 MCP index 或 fallback index
   - `query <q>`：执行结构化 codebase 查询
   - `rebuild`：清理/重建索引

### 验收用例

```bash
sdd codebase doctor --json
```

期望：

- 返回 codebase 专属结果，不是普通 `sdd status`
- MCP 不可用时必须包含：
  - `provider: fallback-file-scan`
  - `degraded: true`
  - 明确提示执行 `sdd codebase doctor`
  - 明确提示检查 `codebase-memory-mcp`

---

## P0-3：`codebase-memory-mcp` 没有真正接入 Core 主链路

### 现象

`Core` 默认构造的是：

```ts
this.codebase = dependencies.codebase ?? new CodebaseAdapter();
```

没有传入任何 MCP transport。

`CodebaseAdapter.initialize()` 在没有 transport 时直接 fallback。

仓库里虽然有 `packages/codebase-memory`，但它没有被 CLI 默认 Core 主链路使用。

`CodebaseMemoryManager.query()` 也只是返回 fallback：

```ts
// 正常模式 — 此处后续可通过 MCP stdio 发送 query 请求
// 当前 MVP 返回 fallback 结果
return fallbackQuery(input);
```

MCP lifecycle 当前只是启动：

```ts
npx -y codebase-memory-mcp@${version}
```

然后 1 秒后未崩溃就判定 `STARTED`，没有完成 MCP protocol 初始化、tools/list、可调用性验证。

### 影响

README 声称：

> `sdd init` 会自动通过 `npx` 启动托管的 `codebase-memory-mcp`。MCP 不可用时会自动降级为 fallback-file-scan，且绝不静默降级。

相关 README 位置。

但当前默认行为更接近：

```text
永远没有 MCP transport → 永远 fallback
```

### 修复要求

1. 实现真实的 `CodebaseMemoryTransport implements McpTransport`。
2. CLI 默认构造 Core 时注入该 transport。
3. transport 至少实现：
   - `isAvailable()`
   - `index(root)`
   - `summarize(root)`
   - `inspect(root)`
   - `capabilities(root)`
   - `query(root, input)`

4. `isAvailable()` 不得只看进程是否 1 秒未退出，应验证 MCP 可调用性。
5. `doctor` 必须能区分：
   - npx 不可用
   - 包下载失败
   - MCP 启动失败
   - MCP 启动成功但 protocol 握手失败
   - MCP tools 不完整
   - index 未完成
   - fallback 正常启用

### 验收用例

```bash
sdd init --json
```

MCP 可用时：

```json
{
  "data": {
    "provider": "codebase-memory-mcp",
    "degraded": false
  }
}
```

MCP 不可用时：

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

## P0-4：`sdd build next` / `sdd build complete` 没有实现主协议闭环

### 现象

CLI wrapper 已识别：

```bash
sdd build next
sdd build complete --task <id> --result <path>
```

相关代码位于 `packages/cli/src/commands/build.ts`。

但 Core 的 `runBuild()` 没有处理 `rawArgs.subcommand === "next"` 或 `"complete"`。它直接进入完整 build 流程并调用 `TaskExecutor`。

默认 executor 是 `MissingTaskExecutor`，会直接失败：

```ts
"宿主适配器必须为 sdd build 提供 TaskExecutor";
```

虽然 `AgentActionRequired` 类型已经定义，但主流程没有返回它。

### 影响

三期最关键的 Agent 协议无法闭环：

```text
sdd build next
→ 返回 AGENT_TASK_EXECUTION
→ Agent 执行任务
→ 写 result.json
→ sdd build complete --task --result
→ Core 验收任务并推进状态
```

### 修复要求

在 `runBuild()` 开头增加 subcommand 分发：

```ts
if (rawArgs?.subcommand === "next") {
  return buildNext(root, state, changeId, tasks);
}

if (rawArgs?.subcommand === "complete") {
  return buildComplete(root, state, changeId, rawArgs);
}
```

`build next` 应：

- 读取当前 change 的 `tasks.json`
- 找到第一个可执行且未完成任务
- 返回 `CommandResult.actionRequired`
- 不调用 `TaskExecutor`
- 不修改业务代码
- 可将任务状态置为 `BUILDING`，但必须支持恢复/重试

返回结构示例：

```json
{
  "ok": true,
  "state": "BUILDING",
  "exitCode": 0,
  "actionRequired": {
    "type": "AGENT_TASK_EXECUTION",
    "taskId": "T001",
    "changeId": "change-xxx",
    "contextPack": ".sdd/context-packs/change-xxx/T001.md",
    "allowedFiles": [],
    "expectedNewFiles": [],
    "forbiddenFiles": [],
    "verification": [],
    "resultFile": ".sdd/runs/run-xxx/tasks/T001.result.json",
    "codebase": {
      "provider": "codebase-memory-mcp",
      "degraded": false
    }
  }
}
```

`build complete` 应：

- 读取 `--task`
- 读取 `--result`
- 校验 result schema
- 校验文件范围
- 校验 TDD/verification evidence
- 写入 `.sdd/changes/<changeId>/task-results.json`
- 更新 `.sdd/state.json.tasks[taskId] = DONE`
- 所有任务完成后进入 `BUILD_READY`
- 未完成时保持 `BUILDING` 或 `PLAN_READY` 语义一致，继续允许 `build next`

### 验收用例

```bash
sdd build next --json
```

期望返回 `actionRequired.type = AGENT_TASK_EXECUTION`。

```bash
sdd build complete --task T001 --result .sdd/runs/run-xxx/tasks/T001.result.json --json
```

期望：

- 单任务验收
- 状态正确推进
- 不依赖 `MissingTaskExecutor`

---

## P0-5：CLI 参数解析会拦截 `--task` / `--result`

### 现象

CLI 顶层 `parseArgs()` 只声明了全局参数：

```ts
json;
cwd;
change;
timeout;
non - interactive;
force;
verbose;
help;
version;
```

但 `build complete` 使用：

```bash
--task
--result
```

CLI 后面虽然尝试从 `positionals` 查找 `--task` 和 `--result`。

问题是 `node:util.parseArgs()` 默认 `strict: true`，未知参数会直接抛错。因此 `--task` / `--result` 可能在进入后续逻辑前就被拦截。

### 修复要求

采用二阶段解析或关闭 strict。

推荐：

```ts
const { values, positionals } = parseArgs({
  args: process.argv.slice(2),
  options: {
    json: { type: "boolean", default: false },
    cwd: { type: "string" },
    change: { type: "string" },
    timeout: { type: "string" },
    "non-interactive": { type: "boolean", default: false },
    force: { type: "boolean", default: false },
    verbose: { type: "boolean", default: false },
    help: { type: "boolean", default: false },
    version: { type: "boolean", default: false },
    task: { type: "string" },
    result: { type: "string" },
    host: { type: "string" },
    structurePolicy: { type: "string" },
  },
  allowPositionals: true,
});
```

或者：

```ts
strict: false;
```

并自行校验未知参数。

---

# P1 高风险问题

## P1-1：`--non-interactive` 没有映射到 Core 预期字段

### 现象

CLI 传入：

```ts
extraArgs["non-interactive"] = true;
```

但 `runNew()` 读取的是：

```ts
nonInteractive;
```

### 影响

`--non-interactive` 对 blocker 问题逻辑无效。

### 修复要求

CLI 改为：

```ts
if (values["non-interactive"]) extraArgs.nonInteractive = true;
```

Core 解析层兼容旧字段：

```ts
const nonInteractive =
  args.nonInteractive === true || args["non-interactive"] === true;
```

---

## P1-2：Agent Protocol 与 Core Build Protocol 版本不一致

### 现象

`packages/agent-protocol` 定义：

```ts
schemaVersion: "1.0.0";
status: "DONE" | "FAILED" | "SKIPPED";
```

校验器也强制：

```ts
schemaVersion 必须为 1.0.0
```

但 Core build 的 v2 协议是：

```ts
schemaVersion: "1.2.0";
status: "SUCCEEDED" | "FAILED" | "BLOCKED" | "SKIPPED" | "DEGRADED";
commandEvidence;
fileDelta;
timestamps;
```

Core normalizer 也按 `schemaVersion === "1.2.0"` 判断 v2 结果。

### 影响

Agent 按 `packages/agent-protocol` 写出的结果，和 Core build 验收格式不一致。

### 修复要求

统一协议事实源：

1. 将 `packages/agent-protocol` 升级到 `schemaVersion: "1.2.0"`。
2. 状态枚举与 Core 对齐：
   - `SUCCEEDED`
   - `FAILED`
   - `BLOCKED`
   - `SKIPPED`
   - `DEGRADED`

3. 复用或导出 Core 的 `TaskExecutionResultV2`。
4. `validateTaskResult()` 应校验 1.2.0 结构。
5. 如果需要兼容 1.0.0，应提供显式 migration，不应静默混用。

---

## P1-3：Claude command 安装缺少 `codebase`

### 现象

`installClaudeCommands()` 按 Core 的 `COMMANDS` 生成 `.claude/commands/sdd.<command>.md`。

但 Core 的 `COMMANDS` 不包含 `codebase`。

### 影响

`/sdd.codebase` 不会被安装，Agent 文档和 README 中的 codebase 命令不一致。

### 修复要求

修复 P0-2 后，`COMMANDS` 包含 `codebase`，安装逻辑自然生成对应命令。

也可以单独为 codebase 子命令生成：

```text
.claude/commands/sdd.codebase.status.md
.claude/commands/sdd.codebase.doctor.md
.claude/commands/sdd.codebase.index.md
.claude/commands/sdd.codebase.query.md
.claude/commands/sdd.codebase.rebuild.md
```

---

# P2 工程与发布问题

## P2-1：当前没有 GitHub Packages 发布配置

### 现状

根 `package.json`：

```json
"name": "sdd-harness",
"private": true
```

CLI 子包：

```json
"name": "@sdd-harness/cli"
```

README 当前写的是“不发布 npm”，安装脚本通过 `npm link` 注册命令。

当前 CI 只有格式、lint、typecheck、test、schema 校验，没有 publish workflow。

### 修复建议

在主链路修复前，不建议发布。

主链路修复后，如果要发布 GitHub Packages，建议新增：

```text
.github/workflows/npm-publish-github-packages.yml
```

并决定发布策略：

方案 A：发布多个 workspace 包：

```text
@sdd-harness/cli
@sdd-harness/core
@sdd-harness/agent-protocol
@sdd-harness/codebase-memory
```

方案 B：改成用户入口包：

```text
@liuyi-it/sdd-harness
```

更推荐方案 B，降低用户安装复杂度。

---

## P2-2：`package-lock.json` 被忽略，CI 不可复现

### 现象

`.gitignore` 忽略了：

```text
package-lock.json
```

CI 使用：

```yaml
npm install
```

### 修复建议

1. 提交 `package-lock.json`
2. CI 改为：

```yaml
- run: npm ci
```

---

# 建议修复顺序

请按以下顺序修：

1. 修复 CLI 参数映射：
   - `--change -> changeId`
   - `--non-interactive -> nonInteractive`
   - 支持 `--task` / `--result`

2. 修复 `runNew()`：
   - 未传 `changeId` 时自动生成
   - `sdd new` / `sdd auto` 主流程可跑通

3. 把 `codebase` 纳入 Core command：
   - 实现 `sdd codebase status/doctor/index/query/rebuild`

4. 实现 `build next` / `build complete`：
   - 返回 `AGENT_TASK_EXECUTION`
   - 消费 Agent result
   - 单任务验收并推进状态

5. 统一 Agent Protocol 到 `1.2.0`
6. 真正接入 `CodebaseMemoryManager` 到 `CodebaseAdapter`
7. 补充测试
8. 最后再处理 GitHub Packages 发布

---

# 必须补充的测试

## CLI 参数测试

覆盖：

```bash
sdd new "xxx"
sdd new "xxx" --change change-001
sdd auto "xxx"
sdd new "xxx" --non-interactive
sdd build complete --task T001 --result result.json
```

## Codebase 命令测试

覆盖：

```bash
sdd codebase status
sdd codebase doctor
sdd codebase index
sdd codebase query "OrderService"
sdd codebase rebuild
```

要求验证它们不是普通 `status` 结果。

## Build Agent Protocol 测试

覆盖：

```bash
sdd build next --json
```

必须返回：

```json
{
  "actionRequired": {
    "type": "AGENT_TASK_EXECUTION"
  }
}
```

覆盖：

```bash
sdd build complete --task T001 --result result.json --json
```

必须能：

- 校验 result schema
- 校验 allowedFiles
- 校验 TDD evidence
- 更新 task 状态
- 所有任务完成后进入 `BUILD_READY`

## MCP fallback 测试

模拟 MCP 不可用，期望：

- fallback 到 `fallback-file-scan`
- `degraded: true`
- warnings 中必须包含 `sdd codebase doctor`
- `.sdd/index/codebase-diagnostics.json` 存在

---

# 当前结论

三期当前还不能发布，也不建议交给真实 Agent 使用。

主要原因是：

1. `sdd new` / `sdd auto` 主流程会因为 changeId 问题失败。
2. `sdd codebase ...` 子命令没有真正执行。
3. `codebase-memory-mcp` 没有真正接入默认 Core 主链路。
4. `sdd build next` / `sdd build complete` 没实现 Agent 协议闭环。
5. CLI 参数解析会拦截关键子命令参数。
6. Agent Protocol 与 Core Build Protocol 版本不一致。

修完上述 P0 后，再进入发布流程。
