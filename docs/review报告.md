# sdd-harness 最新代码 Review 报告

## 一、Review 结论

**结论：REQUEST_CHANGES。**

本次提交修复了上一轮多数状态机问题，包括：

- `build complete` 阶段准入；
- waiting task 一致性校验；
- TDD evidence 深度校验；
- 损坏 `task-results.json` 不再静默覆盖；
- 制品恢复时的任务完成判断；
- ABORTED run 禁止普通 auto 复用；
- resume 时同步更新 LoopRun；
- MCP fallback 状态传递；
- MCP 版本统一为 `0.9.0`。

但本轮发现两个新的阻断级回归：

1. **手动执行 `sdd build next` 后无法执行 `sdd build complete`。**
2. **符合当前 V2 类型定义的合法结果会被 `parseCompleteResult()` 拒绝。**

此外，codebase-memory-mcp 的真实查询仍未实现，当前“正常 MCP 模式”实际仍执行 fallback query。

---

# 二、阻断级问题

## P0-1：手动 `build next → build complete` 流程被严格校验破坏

### 问题

`build next` 只在 `activeLoop` 已存在时写入 waiting：

```ts
activeLoop:
  current.activeLoop !== null && typeof current.activeLoop === "object"
    ? {
        ...current.activeLoop,
        status: "WAITING_AGENT",
        waiting: { ... }
      }
    : current.activeLoop
```

如果用户通过普通手动流程执行：

```bash
sdd init
sdd new ...
sdd design
sdd plan
sdd build next
```

此时通常没有 `activeLoop`，所以 `activeLoop` 仍是 `null`，但命令仍返回了 `AgentActionRequired`。

新版 `build complete` 又严格要求：

```ts
activeLoop.status === "WAITING_AGENT"
waiting.reason === "AGENT_TASK_EXECUTION"
waiting.taskId === taskId
waiting.resultFile 存在
state.tasks[taskId] === "BUILDING"
```

因此前一步通过手动 `build next` 取得的任务必然无法 complete。

重复执行 `build next` 也无法返回同一 handoff，因为没有保存 waiting；已分配任务会停留在 `BUILDING`，随后只返回 `pendingBuild`。

### 修改建议

不能让手动 build handoff 依赖 auto 专属的可选对象。

推荐增加独立状态字段：

```ts
pendingAgentTask: {
  taskId: string;
  resultFile: string;
  since: string;
} | null;
```

`build next` 无论是否处于 auto loop，都写入该字段；`activeLoop.waiting` 仅作为 auto loop 的镜像。

最小修复方案是在 `activeLoop === null` 时创建：

```ts
activeLoop: {
  loopId: "manual-build",
  runId,
  status: "WAITING_AGENT",
  waiting: {
    reason: "AGENT_TASK_EXECUTION",
    taskId: nextTask.id,
    resultFile,
    since: now,
  },
}
```

但这会混淆 manual build 与 auto loop 语义，独立 handoff 状态更合理。

---

## P0-2：合法 V2 TaskExecutionResult 会被拒绝

### 问题

当前 V2 类型把元数据放在外层：

```ts
interface TaskExecutionResultV2 {
  schemaVersion: "1.2.0";
  taskId?: string;
  status: ...;
  fileDelta: ...;
  legacy?: TaskExecutionResult;
}
```

而 `legacy` 只包含：

```ts
modifiedFiles;
tddEvidence;
verification;
```

不包含 `schemaVersion`、`taskId` 和 `status`。

Normalizer 生成的 V2 制品也明确把 `taskId/status/schemaVersion` 放在外层，`legacy` 保存原始 v1 result。

但新版 `parseCompleteResult()` 在识别到 V2 后，把 `result.legacy` 赋给 `parsed`，随后要求 `parsed` 自身包含：

```ts
parsed.schemaVersion;
parsed.taskId;
parsed.status;
```

这意味着由项目自身 `normalizeTaskExecutionResult()` 生成的标准 V2 文件无法被 `build complete` 接受。

### 修改建议

分别解析 envelope 和 legacy：

```ts
const envelope = taskResultV2Schema.parse(value);
const legacy = taskResultV1EvidenceSchema.parse(envelope.legacy);

return {
  ...legacy,
  schemaVersion: envelope.schemaVersion,
  taskId: envelope.taskId,
  status: envelope.status,
};
```

不要要求 legacy 重复 envelope 的字段。

应新增 round-trip 测试：

```ts
const artifact = normalizeTaskExecutionResult(...);
const result = await buildComplete(artifact);
expect(result.ok).toBe(true);
```

---

## P0-3：MCP 查询仍然没有真实调用 codebase-memory-mcp

`CodebaseMemoryManager.query()` 在 MCP 正常模式下仍直接执行：

```ts
return fallbackQuery(input);
```

代码注释也明确写着“当前 MVP 返回 fallback 结果”。

但 capabilities 在进程存活时仍宣称支持：

- symbols
- callers
- callees
- routes
- tests
- architecture
- graph query

### 影响

当前状态是：

- MCP 进程可能被启动；
- 系统显示 provider 为 `codebase-memory-mcp`；
- 但所有 query 实际仍是 fallback；
- graph query 能力并不存在。

### 修改建议

在真实完成以下链路前，不应声明 MCP 查询能力可用：

1. MCP `initialize`。
2. `initialized` notification。
3. `tools/list`。
4. 真实 `tools/call`。
5. 返回内容 Schema 校验。
6. 协议失败后降级 fallback。

---

# 三、高优先级问题

## P1-1：MCP 成功后 diagnostics 仍可能全部为 false

`CodebaseAdapter.initialize()` 在启动 MCP 之前调用：

```ts
const inspected = await transport.inspect(root);
```

此时 manager 尚无 lifecycle，transport 会报告未连接、不可调用、未索引。

MCP 启动成功后，adapter 构造 diagnostics 时把 `inspected` 放在最后：

```ts
{
  installed: true,
  configured: true,
  connected: true,
  callable: true,
  indexed: true,
  ...inspected
}
```

旧的 false 值会覆盖前面的 true。

Transport 的 inspect 确实会在初始化前根据空 lifecycle 返回全部 false。

### 影响

MCP 实际启动成功，但：

```bash
sdd codebase doctor
```

仍可能报告五项检查全部失败。

### 修改建议

初始化成功后重新 inspect：

```ts
await transport.index(root);
const inspectedAfterInit = await transport.inspect?.(root);
```

或者不要让初始化前的 snapshot 覆盖已确认的成功状态。

---

## P1-2：`requireAvailable=true` 或禁用 fallback 时仍可能被报告为成功

Manager 在以下两种失败情况下返回 `degraded: false`：

- `requireAvailable=true`；
- fallback 被禁用。

Transport 只把 `degraded` 传给 adapter。

Adapter 又把 `degraded === false` 解释成初始化成功，并返回：

```ts
provider: "codebase-memory-mcp",
degraded: false
```

### 修改建议

`InitResult` 不能只使用一个 degraded boolean，建议改为：

```ts
status: "AVAILABLE" | "DEGRADED" | "FAILED";
```

语义：

- `AVAILABLE`：MCP 可用；
- `DEGRADED`：MCP 不可用，fallback 正常；
- `FAILED`：MCP 不可用且不允许 fallback。

`FAILED` 应抛出 `E_COMPONENT_UNAVAILABLE`，不能进入正常初始化路径。

---

## P1-3：非成功任务结果被当作成功命令处理

当前允许 Agent 提交：

```text
FAILED
BLOCKED
SKIPPED
DEGRADED
```

这些状态会被映射成任务 `FAILED`，但 `build complete` 最终仍返回：

```ts
ok: true;
state: "PLAN_READY";
next: "sdd build next";
```

Auto Loop 会把这次 complete 当作成功步骤，并可能立即重新分配同一个 FAILED 任务。LoopSpec 中的 retry 限制目前又没有实际执行。

### 修改建议

仅 `status === "SUCCEEDED"` 可以作为成功 complete。

其他状态应返回结构化失败，例如：

```ts
return {
  ok: false,
  state: "FAILED",
  exitCode: 7,
  error: {
    code: "E_AGENT_TASK_FAILED",
    message: `任务 ${taskId} 返回状态 ${status}`,
  },
};
```

需要重试时，由 `auto --resume`、显式 retry 或 LoopPolicy 决定，不能通过 `ok: true` 隐式重试。

---

## P1-4：`readResults()` 仍没有验证结果元素

新版已经修复“损坏 JSON 被当成文件不存在”的问题，这是正确的。

但 `readResults()` 目前只验证顶层是数组，然后直接：

```ts
return parsed as TaskResult[];
```

如果文件内容是：

```json
[null]
```

后续访问 `result.taskId` 时仍会产生普通 TypeError。

项目已有 `parseTaskResults()`，能够校验 taskId、modifiedFiles、TDD evidence 和 verification。

### 修改建议

```ts
async function readResults(path: string): Promise<TaskResult[]> {
  try {
    return parseTaskResults(await readFile(path, "utf8"));
  } catch (error) {
    if (isMissingFile(error)) return [];
    throw error;
  }
}
```

---

## P1-5：Task Schema 只在 `build complete` 使用

`build complete` 已改用 `parseTasks()`，这是改进。

但以下路径仍直接 `JSON.parse() as TaskDefinition[]`：

- 传统 `sdd build`；
- `sdd build next`。

因此相同的 `tasks.json` 在三个入口存在不同安全等级。

另外，`parseTasks()` 检查了重复 ID、自依赖和缺失依赖，但没有检查完整依赖图是否有环。

### 修改建议

所有读取 `tasks.json` 的路径统一调用 `parseTasks()`，并在 parser 中加入 DAG cycle 检查。

同时补充：

- allowedFiles 安全相对路径；
- expectedNewFiles 安全相对路径；
- forbiddenFiles pattern 校验；
- verification 命令 allowlist 校验。

---

## P1-6：传统 build 的结果无法正确参与制品恢复

传统 `TaskExecutionResult` 没有 `status` 字段。

传统 build 会把这些 legacy result 直接写入 `task-results.json`。

但新版 `allTasksDone()` 要求每条结果包含：

```ts
status === "DONE" || status === "SUCCEEDED";
```

因此传统 build 全部完成后，如果 state 和 backup 损坏，制品恢复仍可能把阶段降回 `PLAN_READY`，而不是 `BUILD_READY`。

### 修改建议

所有 build 模式统一持久化 canonical result：

```ts
{
  taskId,
  status: "DONE",
  modifiedFiles,
  tddEvidence,
  verification
}
```

不要让传统 build 和 `build complete` 写入两种不同 shape。

---

## P1-7：`activeLoop` 仍是 `z.unknown()`

Workflow Schema 中仍然是：

```ts
activeLoop: z.unknown().nullable();
```

这正是 manual handoff、resume、waiting 和 stop 状态容易不一致的根本原因之一。

### 修改建议

增加正式的 `activeLoopSchema` 和 `waitingSchema`，并使用 refine 约束：

- `WAITING_AGENT` 必须存在 waiting；
- `AGENT_TASK_EXECUTION` 必须有 taskId 和 resultFile；
- ABORTED/SUCCEEDED 不应有 waiting；
- runId/loopId 使用安全 ID；
- waiting task 必须和 `state.tasks[taskId] === BUILDING` 一致。

---

# 四、已确认修复的项目

## 1. `build complete` 已增加严格状态准入

现在只有 `BUILD_WAITING_AGENT` 可以 complete，并会检查 waiting reason、taskId、resultFile 和任务状态。

该修复方向正确，只是遗漏了 manual build handoff。

## 2. TDD evidence 已复用统一质量规则

`build complete` 现在调用 `taskEvidenceFailures()`，能够验证 RED 预期失败、阶段匹配和 VERIFY evidence。

## 3. `task-results.json` 写入已改为临时文件加 rename

结果文件不再直接覆盖，降低了半写入风险。

## 4. 制品恢复的 allTasksDone 判断已明显增强

现在会检查：

- tasks 非空；
- taskId 唯一；
- results 数量一致；
- result 状态成功；
- 每个 task 都有对应 result。

## 5. Loop 生命周期修复基本正确

已修复：

- FAILED/PAUSED 不再被错误改回 RUNNING；
- ABORTED run 禁止普通 auto 复用；
- resume 会把指定 run 改回 RUNNING、清除 endedAt 并写入 LOOP_RESUMED；
- WAITING_AGENT/PAUSED 不再写 endedAt。

---

# 五、测试审查

本次提交修改了核心状态机、MCP 和 result parser，但变更清单中没有测试文件。

现有 `build.test.ts` 主要覆盖传统完整 build，没有看到以下关键场景：

- 手动 `build next → build complete`；
- 重复 `build next` 返回同一 handoff；
- V2 normalized artifact round-trip；
- 在错误 phase 执行 complete；
- waiting 缺失或不匹配；
- existingResults 包含 null/畸形条目；
- Agent 返回 FAILED/BLOCKED/DEGRADED；
- 传统 build 完成后的 state recovery。

现有 auto 测试覆盖 resume/restart 和推进到 handoff，但未覆盖 manual build handoff，也未覆盖历史 waiting run 恢复。

当前提交没有查询到 GitHub Actions workflow run。

我也尝试拉取仓库执行本地测试，但当前执行环境无法解析 `github.com`，所以没有独立的运行结果。

---

# 六、建议修复顺序

1. 修复 manual `build next/complete` handoff 持久化。
2. 修复 V2 envelope/legacy 解析。
3. 为上述两个问题增加回归测试。
4. 让非 SUCCEEDED result 返回失败。
5. 所有 tasks/results 读取统一使用 canonical parser。
6. 修复 MCP 初始化后 diagnostics。
7. 区分 MCP AVAILABLE/DEGRADED/FAILED。
8. 实现真实 MCP initialize/tools/list/tools/call。
9. 统一传统 build 与 handoff build 的持久化结果格式。
10. 将 activeLoop 纳入 Zod Schema。

---

# 七、最终判定

| 项目                         | 状态           |
| ---------------------------- | -------------- |
| 上轮 build complete 阶段绕过 | 已修复         |
| Result 元素类型校验          | 基本修复       |
| TDD evidence 校验            | 已修复         |
| 损坏 result 文件处理         | 部分修复       |
| 制品恢复误判                 | 基本修复       |
| Loop stop/resume 生命周期    | 基本修复       |
| Manual build handoff         | 阻断级回归     |
| V2 result 兼容               | 阻断级回归     |
| MCP fallback 状态            | 默认场景已改善 |
| MCP 真实查询                 | 未完成         |
| 自动化测试证据               | 不足           |

**最终 Review 意见：暂不通过。**

完成 manual handoff 和 V2 round-trip 两项修复后，工作流核心才能进入下一轮验收；MCP 真实查询闭环仍应作为三期遗留阻断项单独关闭。
