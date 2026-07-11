# sdd-harness Release Gate 全量 Review 与修复目标

## 1. 审查基线

- 仓库：`liuyi-it/sdd-harness`
- 分支：`main`
- 基线提交：`2077668088ee1d883319d94d0bef3392275c2c38`
- 提交说明：`fix: 完善生产级工作流与 MCP 查询闭环`
- 运行时要求：Node.js 22+
- 产品约束：
  - 保留 `sdd auto`，不增加 `loop` 命令；
  - 主流程应尽量不要求用户输入；
  - 不发布 npm，但必须保证本地安装和打包可用；
  - 支持手动命令模式和自动 Loop Engine 模式。

当前 GitHub 没有可用于证明该提交通过完整 CI 的 workflow run。

## 2. 当前结论

**Review 结论：REQUEST_CHANGES。**

当前版本已完成以下重要修复：

- 手动 `build next` handoff 可持久化并重复返回；
- 标准 V2 TaskExecutionResult 可以完成 round-trip；
- 非成功 Agent 结果会阻断流程；
- Tasks 和 task-results 有较完整的运行时校验；
- `activeLoop` 和 `pendingAgentTask` 已纳入状态 Schema；
- MCP 已开始执行 initialize、tools/list、index_repository 和 tools/call。

但仍有生产级阻断项：

| 级别 | 数量 | 说明                                 |
| ---- | ---: | ------------------------------------ |
| P0   |    4 | 安全边界、MCP 真实性和生命周期       |
| P1   |   10 | 状态一致性、协议契约、升级兼容和并发 |
| P2   |    5 | 工程质量、可维护性和诊断体验         |

在所有 P0 和 P1 关闭前，不应标记四期最终验收通过。

---

# 3. 产品最终目标

`sdd-harness` 的目标不是一组松散 CLI，而是一个可恢复、可审计、确定性的自动软件交付状态机。

最终用户应能够执行：

```bash
sdd auto "实现某项需求"
```

系统自动完成：

```text
init
→ codebase index
→ new/spec
→ design
→ plan
→ build handoff
→ Agent 实施
→ build complete
→ verify
→ review
→ archive
```

仅在以下情况暂停：

1. 需求存在无法安全推断的关键歧义；
2. 需要外部 Coding Agent 执行任务；
3. 明确配置了人工审核门禁；
4. 出现不可自动恢复的失败。

任何失败都必须满足：

- 不破坏已有制品；
- 状态可解释；
- 有结构化错误码；
- 有明确恢复命令；
- 重复执行不会产生不同副作用；
- 不会将 fallback 伪装成精确结果；
- 不会信任 Agent 自报内容作为唯一安全依据。

---

# 4. 必须保留的已修复行为

Codex 修改时不得回退以下能力。

## NR-001：不得重新引入 FileLock 自锁

普通 `auto` 不得在外层长时间持有仓库锁，再调用内部也需要锁的命令。

## NR-002：手动 handoff 必须可用

以下流程必须保持可用：

```bash
sdd build next
sdd build next
sdd build complete --task <task> --result <result>
```

第二次 `build next` 必须返回相同 taskId 和 resultFile。当前已有对应测试。

## NR-003：V2 result 必须兼容

V2 envelope 的 taskId/status/schemaVersion 与 legacy evidence 必须组合解析，不能要求 legacy 重复 envelope 字段。当前实现已按该方式处理。

## NR-004：非成功 Agent 结果必须失败

`FAILED/BLOCKED/SKIPPED/DEGRADED` 不得返回 `ok: true`。当前实现会进入 `E_AGENT_TASK_FAILED`。

## NR-005：状态对象不得退回 unknown

`activeLoop`、waiting 和 `pendingAgentTask` 必须继续使用正式 Schema。

## NR-006：Tasks 依赖图必须校验

必须继续校验重复 ID、非法依赖、环依赖、危险路径和危险验证命令。

---

# 5. P0：发布阻断问题

## P0-1：外部 Agent handoff 没有用真实 Git delta 裁决文件范围

传统 `sdd build` 会在执行前后获取 Git snapshot，并使用真实 delta 判断实际修改文件。

但 `build complete` 只验证 Agent 自报的 `modifiedFiles`。

因此 Agent 可以修改越权文件，却提交：

```json
{
  "modifiedFiles": []
}
```

### 必须实现

`build next` 时保存：

```ts
pendingAgentTask: {
  taskId: string;
  resultFile: string;
  since: string;
  runId: string;
  businessRoot: string;
  gitBaseline: GitSnapshot;
}
```

`build complete` 时：

1. 获取当前 Git snapshot；
2. 计算真实 added/modified/deleted；
3. 使用真实 delta 校验 allowedFiles、expectedNewFiles、forbiddenFiles；
4. 检测 Agent 声明和真实 delta 的差异；
5. 未申报文件变化返回 `E_UNDECLARED_FILE_CHANGE`；
6. 越权变化返回 `E_SECURITY_BLOCKED`；
7. 最终 artifact 以真实 Git delta 为准；
8. 删除文件也必须纳入校验；
9. worktree 模式下必须检查业务 worktree，而非 control root。

Agent 自报的文件列表只能作为辅助证据，不能成为事实源。

---

## P0-2：MCP 工具发现、fallback 和结果身份不真实

当前只执行一次 `tools/list`。

固定上游实现支持分页，第一页不是完整工具列表；`detect_changes` 位于后续工具列表中。

Manager 发现缺少工具时会 fallback。

但 Core 的 `queryImpact()` 会丢弃 fallback 身份，重新包装成：

```text
provider=codebase-memory-mcp
degraded=false
confidence=0.8
```

### 必须实现

- 遍历全部 `tools/list` 分页；
- 对 cursor 做循环和重复 cursor 防护；
- 保存真实 availableTools；
- 从真实工具清单推导 supportedIntents；
- MCP 返回 fallback 时保留：
  - provider；
  - degraded；
  - reason；
  - confidence；

- 禁止 Core 二次包装后改变结果身份；
- `queryImpact()` 和通用 `query()` 使用同一结果契约；
- fallback 结果必须携带真实扫描结果，不能被替换成空数组。

---

## P0-3：MCP 进程生命周期不可靠

当前 `timeoutMs` 被同时用于协议请求和 `spawn()` 进程配置。

MCP 是长期运行进程，不能使用启动请求超时作为整个进程生命周期限制。

此外，进程退出后，Manager 保存的 lifecycle status 仍可能是 `STARTED`，导致：

- `isAvailable()` 返回 true；
- capabilities 继续报告 MCP 可用；
- diagnostics 与实际进程不一致。

### 必须实现

- 删除进程级 `spawn.timeout`；
- timeout 只用于单次 JSON-RPC 请求；
- initialize、tools/list、index、query 都有独立请求超时；
- 子进程 exit/error 时立即：
  - 标记 session closed；
  - 更新 lifecycle status；
  - 拒绝所有 pending request；
  - 将 capabilities 标记为 degraded；

- 每次 query 前确认进程和 session 可用；
- query 失败后按策略降级；
- stop 必须清理强制终止 timer；
- 不得遗留孤儿进程；
- 同一 Manager 不得重复启动多个 MCP 进程。

---

## P0-4：MCP 成功路径生成的代码库摘要仍是空壳

当前 MCP 成功时 summarize 返回：

```text
codebaseSummary = “已索引”
packageStructure = ""
architecture = "mcp-managed"
```

这不足以支持 planner 可靠识别：

- 源码文件；
- 测试文件；
- 模块结构；
- 构建命令；
- 架构边界。

### 必须实现

采用“基础扫描 + MCP 增强”策略：

1. fallback-file-scan 始终生成基础摘要；
2. MCP 成功后补充：
   - 项目结构；
   - 主要模块；
   - entrypoints；
   - 关键 symbols；
   - routes；
   - tests；
   - architecture；
   - build/test commands；

3. MCP 结果为空时继续保留基础扫描内容；
4. codebase summary 中明确记录 provider 和 degraded；
5. planner 不得因为 MCP 返回空结果而失去基础上下文。

---

# 6. P1：本期必须关闭的问题

## P1-1：CLI query 文本在传输过程中丢失

CLI 构造 `{intent, query}`，但 Core 输入类型没有 `query` 字段，Transport 最终使用 `requirement ?? intent`。

### 目标

统一为：

```ts
interface McpQueryInput {
  intent: McpQueryIntent;
  query: string;
  root: string;
  changeId?: string;
  hint?: QueryHint;
}
```

删除所有为绕过类型系统而添加的强制断言。

---

## P1-2：capabilities 混淆 tools 和 intents

Transport 返回 supportedIntents，Core 却将其当成工具名称写入 availableTools。

### 目标

定义唯一 canonical capability：

```ts
interface CanonicalMcpCapabilities {
  provider: string;
  version: string;
  commit: string;
  availableTools: string[];
  supportedIntents: McpQueryIntent[];
  supportsIndex: boolean;
  supportsGraphQuery: boolean;
}
```

任何层不得根据 intent 名称反推工具。

---

## P1-3：MCP tool result 缺少严格错误处理

JSON-RPC 调用成功不代表 MCP tool 成功。必须检查：

- `isError`;
- `structuredContent`;
- `content`;
- 必填字段；
- 返回 Schema；
- 空结果和无法识别结果。

`index_repository` 返回 `isError=true` 时不能标记 AVAILABLE。

---

## P1-4：MCP 结果解析过于通用

当前 `extractItems()` 只识别 `results/items`，主要生成 file/symbol。

### 目标

按 intent/tool 实现独立 decoder：

```text
decodeSearchGraph
decodeSearchCode
decodeTracePath
decodeDetectChanges
decodeArchitecture
decodeIndexStatus
```

必须正确产生：

- file；
- symbol；
- route；
- test；
- module；
- config；
- risk。

未知成功响应不得静默变成空结果。

---

## P1-5：fallback 查询结果被 Transport 丢弃

Manager fallback 已返回 items，但 Transport 在 degraded 分支重新构造空 payload。

### 目标

fallback 的文件、符号、测试和风险结果必须完整向上传递。

---

## P1-6：restart/stop 没有正确处理 pending handoff

restart 创建新 run 时没有明确迁移或清除旧 `pendingAgentTask`。

stop 终止 loop 时也没有明确处理 pending handoff。

### 目标语义

#### stop

- run 设为 ABORTED；
- pendingAgentTask 保留为“暂停的人工任务”或清空，必须二选一并文档化；
- 不允许处于“ABORTED run + 可继续提交结果”的模糊状态。

推荐：stop 清空 pending，并将 BUILDING task 恢复为 PENDING。

#### restart

- 旧 run ABORTED；
- 旧 handoff 失效；
- BUILDING task 恢复 PENDING；
- 创建新 run；
- 新 build next 生成新 resultFile；
- 不得返回旧 run 的 resultFile。

---

## P1-7：恢复历史 waiting run 不完整

当前 resume 只在恢复当前 active run 时保留 waiting。

### 目标

恢复非当前 waiting run 时：

- 从 LoopRun steps 或 event log 找到最后一次有效 AGENT_HANDOFF；
- 重建 pendingAgentTask；
- 校验 resultFile 和 taskId；
- 无法可靠恢复时明确拒绝；
- 不得静默生成无 waiting 的 RUNNING loop。

---

## P1-8：State Schema 版本未随不兼容结构变化升级

当前新增 pendingAgentTask 和 activeLoop refine，但 schemaVersion 仍为 `1.3.0`。

旧 1.3.0 waiting state 可能无法通过新 Schema。

### 目标

升级到 `1.4.0`，实现：

```text
1.0.0 → 1.4.0
1.2.0 → 1.4.0
旧 1.3.0 → 1.4.0
```

迁移必须：

- 备份 `.sdd`；
- 从 activeLoop.waiting 重建 pendingAgentTask；
- 无法唯一确定任务时进入结构化 FAILED；
- 生成迁移报告；
- 支持幂等重复读取。

---

## P1-9：V2 task artifact 持久化有损

当前 V2 被压平为 TaskResult 后，再写入 run result。

可能丢失：

- fileDelta；
- timestamps；
- mode；
- summary；
- commandEvidence；
- notes。

### 目标

解析返回：

```ts
{
  (canonicalResult, validatedOriginalArtifact);
}
```

- `task-results.json` 保存 canonical result；
- run task artifact 保存完整 validated V2；
- Git delta 使用系统真实检测值覆盖；
- round-trip 测试必须比较完整字段，而不只是 `ok: true`。

---

## P1-10：Loop metadata 仍有并发覆盖风险

StateStore update 是 read-modify-write。

普通 auto 不再持有整个全局锁后，多个 auto 进程可能并发追加 step 或更新 activeLoop。

### 目标

增加短生命周期 loop metadata lock 或 version CAS，保护：

- prepareLoop；
- recordStep；
- finalizeLoop；
- resume；
- restart；
- stop；
- handoff 创建和完成。

不得重新使用覆盖整个内部命令执行链的长锁。

---

# 7. P2：应在本轮一并清理

## P2-1：Windows MCP 启动

不得假设 `spawn("npx")` 在 Windows 一定可执行。

要求增加 Windows 兼容启动器和 Windows CI。

## P2-2：MCP pending request 无取消和超时清理

每个 request 超时后必须：

- 从 pending Map 删除；
- 可选发送取消 notification；
- 不得持续占用内存。

## P2-3：任务结果事务一致性

完成 handoff 时，以下写入必须具备可恢复事务语义：

- run artifact；
- task-results.json；
- state.json；
- loop event。

建议使用 commit marker 或 completion journal。

## P2-4：codebase doctor 退出码

严格检查失败时应返回非零退出码，便于 CI 使用。

## P2-5：dist 与源码一致性

构建后必须通过：

```bash
git diff --exit-code
```

确保提交中的 dist 对应当前源码。

---

# 8. 自动 Loop Engine 完整验收标准

## AC-001：零输入主路径

```bash
sdd auto "明确、完整的需求"
```

必须自动推进至 AGENT_TASK_EXECUTION，不要求用户手动执行中间命令。

## AC-002：澄清路径

需求存在关键阻塞歧义时进入 CLARIFYING；提供 answers 后能继续同一个 run。

## AC-003：手动路径

手动执行所有阶段与 auto 得到相同的制品和状态契约。

## AC-004：handoff 幂等

重复 build next 返回同一 handoff，不新建重复任务。

## AC-005：handoff 安全

Agent 的未申报文件、越权文件和删除文件均能被真实 Git delta 检测。

## AC-006：handoff 完成

合法结果原子写入，并清除 pendingAgentTask。

## AC-007：Agent 失败

Agent 返回非成功状态时：

- task=FAILED；
- loop=FAILED 或 PAUSED；
- 返回明确恢复命令；
- 不自动无限重试。

## AC-008：restart

restart 生成新 run，不复用旧 resultFile。

## AC-009：resume

当前和历史 waiting run 都能正确恢复；无法恢复时明确拒绝。

## AC-010：stop

stop 后普通 auto 不会静默继续已 ABORTED run。

## AC-011：并发

两个 auto 同时启动时：

- 不丢 step；
- 不覆盖 event；
- 不重复分配同一 task；
- 一个进程得到结构化并发错误或安全等待。

## AC-012：进程中断

在每个阶段被 kill 后，下一次命令能恢复至最近稳定状态。

## AC-013：状态损坏

state 损坏时按以下顺序恢复：

1. backup；
2. journal；
3. artifacts；
4. 无法恢复时 E_STATE_CORRUPTED。

## AC-014：旧版本迁移

1.0.0、1.2.0 和旧 1.3.0 fixture 均可迁移到 1.4.0。

## AC-015：MCP 正常路径

实际完成：

```text
spawn
→ initialize
→ initialized
→ tools/list 全分页
→ index_repository
→ query tools/call
→ typed decode
```

## AC-016：MCP fallback

MCP 任意阶段失败时：

- provider=fallback-file-scan；
- degraded=true；
- reason 非空；
- 保留真实 fallback 结果。

## AC-017：MCP 强制可用

requireAvailable=true 时不得降级为成功；应返回 E_COMPONENT_UNAVAILABLE。

## AC-018：MCP 生命周期

MCP 运行超过 30 秒后仍可查询；退出后 diagnostics 立即变化。

## AC-019：MCP Windows

Windows Node 22 下可启动、索引、查询、停止。

## AC-020：代码库摘要

无论 MCP 是否可用，都生成可供 planner 使用的：

- file tree；
- package structure；
- architecture；
- source/test candidates；
- build/test commands。

## AC-021：Task Schema

非法 taskId、循环依赖、危险路径、危险命令、重复项均被拒绝。

## AC-022：Result Schema

null、数组元素类型错误、非法 status、缺 evidence、危险命令均返回结构化错误，不抛裸 TypeError。

## AC-023：TDD 链

RED、GREEN、REFACTOR、VERIFY 证据完整，且阶段匹配。

## AC-024：V2 无损

V2 result 完成后仍保留完整 envelope 字段。

## AC-025：归档

archive 前必须 verify/review 通过；archive 后不得通过 build complete 回退阶段。

## AC-026：可观察性

status、status --loop、events、doctor 能准确反映事实状态。

## AC-027：错误码稳定

所有预期失败均返回已定义错误码，不使用模糊内部异常。

## AC-028：路径安全

runId、taskId、changeId、resultFile 和 artifact path 均不能逃逸 `.sdd` 或仓库根目录。

## AC-029：命令安全

shell metacharacter、管道、重定向、命令拼接和未授权程序均被阻断。

## AC-030：发布门禁

所有检查、测试、打包和多平台 smoke test 通过。

---

# 9. 必须新增的测试矩阵

## Build handoff

- 手动 next/next/complete；
- auto next/complete；
- Agent 修改允许文件并正确申报；
- Agent 修改允许文件但不申报；
- Agent 修改 forbidden file；
- Agent 新增越权文件；
- Agent 删除越权文件；
- worktree 中的实际 delta；
- V2 完整字段 round-trip；
- completion 中途崩溃后的恢复。

## Loop

- 两进程并发 auto；
- stop while waiting；
- restart while waiting；
- resume current waiting run；
- resume historical waiting run；
- resume corrupted historical run；
- maxSteps；
- maxRetriesPerStep；
- maxRepeatedFailures；
- repeated Agent failure；
- no-progress loop detection。

## MCP

- initialize 成功；
- initialize 超时；
- tools/list 两页以上；
- 重复 nextCursor；
- index_repository isError；
- query isError；
- structuredContent；
- content text JSON；
- content 非 JSON；
- query timeout；
- 子进程意外退出；
- 运行 60 秒后继续查询；
- fallback 保留 items；
- requireAvailable；
- fallback disabled；
- Windows 启动；
- stop 后无孤儿进程。

## Migration

- 1.0.0；
- 1.2.0 BUILDING；
- 旧 1.3.0 waiting；
- 旧 1.3.0 manual handoff；
- 已损坏 waiting；
- 重复迁移。

## Security

- `../`；
- 绝对路径；
- Windows 反斜杠；
- encoded traversal；
- CR/LF/NUL；
- symlink 相关边界；
- `npm test && rm -rf`；
- 管道和重定向；
- 非允许命令；
- resultFile 指向其他 run。

---

# 10. CI 和发布门禁

必须新增或修复 GitHub Actions，至少覆盖：

```text
ubuntu-latest / Node 22
windows-latest / Node 22
macos-latest / Node 22
```

每个平台执行适用检查：

```bash
npm ci
npm run format:check
npm run lint
npm run typecheck
npm run validate:schemas
npm test
npm run validate:release
npm run build
git diff --exit-code
```

Linux 再执行完整集成和 MCP smoke test。

Windows 必须执行：

- CLI help；
- init；
- build next/complete；
- MCP 启动方式 smoke test。

最后执行本地包 smoke test：

```bash
npm pack
npm install <生成的 tarball>
sdd --version
sdd --help
```

不得通过以下方式让 CI 变绿：

- 删除测试；
- `.skip`；
- 放宽核心断言；
- 捕获并忽略异常；
- 将失败结果改成 warning；
- 在 Windows 跳过核心工作流；
- 将真实 MCP 测试全部替换为 mock。

---

# 11. 完成定义

只有同时满足以下条件，任务才算完成：

1. 所有 P0 已关闭；
2. 所有 P1 已关闭；
3. P2 已关闭或有明确、非发布阻断的记录；
4. AC-001 至 AC-030 全部有自动化证据；
5. 三平台 CI 通过；
6. 完整手动工作流通过；
7. 完整 auto 工作流通过；
8. MCP 正常与 fallback 双路径通过；
9. 状态迁移通过；
10. 没有 skipped test；
11. dist 与源码一致；
12. 无已知裸异常；
13. 无 fallback 身份伪装；
14. 无 Agent 自报文件安全依赖；
15. 最终提交包含一份新的验收报告，列出命令及实际结果。

最终只能输出：

```text
PASS
```

或：

```text
REQUEST_CHANGES
```

不得用“基本通过”“大致完成”“理论可用”替代发布判断。
