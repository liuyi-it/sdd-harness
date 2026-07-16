# 架构说明

## 分层与依赖

```text
Agent Adapter ──> CLI ──> Core ──> State / Artifacts / Git / Quality
                              ├──> Agent Policies / Agent Protocol
                              ├──> SpecEngine / TddEngine
                              └──> codebase-memory adapter
```

- `@sdd-harness/cli`：解析参数、路由命令并渲染 `CommandResult`。
- `@sdd-harness/core`：唯一状态机与质量门禁执行层。
- `@sdd-harness/agent-protocol`：定义 Agent 行动要求、结果和约束结构。
- `@sdd-harness/agent-policies`：按阶段解析 Policy，并生成可校验摘要。
- `@sdd-harness/codebase-memory`：托管 MCP 生命周期、查询、诊断和降级。
- `packages/*-adapter`：把宿主指令翻译为 CLI 调用，不直接修改状态。

Core 通过依赖注入接收 `TaskExecutor` 和 MCP transport，外部 Agent 或工具不能绕过 Core 推进阶段。

## 工作流

```text
init → new → design → plan → build → verify → review → archive
```

每个写命令先获取 `.sdd/lock`，再验证当前阶段和活动变更。失败时持久化 `failedCommand`、`previousPhase`、`inProgressPhase` 与建议命令，用于恢复或人工处理。

`auto` 读取同一状态机并循环调用公开命令。它只自动执行确定性步骤；遇到需求澄清、Agent 编码、失败预算耗尽或人工决策时暂停。

## 规格、计划与 Context Pack

```text
requirement + codebase
  → SpecEngine
  → spec.md + spec.json
  → design.md
  → TddEngine
  → plan.json
  → build next
  → 单任务 Context Pack
```

`spec.json` 是规格事实源，包含 proposal、impact、澄清结果、delta 和 Requirement/Scenario 模型。`plan.json` 是计划事实源，包含任务定义、可读计划、测试计划、上下文摘要和可选的 `dependencies` 决策。

Context Pack 不在 `plan` 阶段批量生成。`build next` 或内置构建执行器只为当前任务创建，并根据规格、计划、源码、项目规则、目录规范和 Policy 摘要自动刷新。

## 构建与质量门禁

任务采用 RED、GREEN、REFACTOR、VERIFY 四阶段链。Core 对 Agent 结果执行以下裁决：

1. 验证 TaskExecutionResult 结构和任务身份。
2. 使用 Git 快照计算实际文件变更。
3. 校验允许文件、期望新增文件和禁止文件。
4. 校验 TDD evidence 与最终 verification。
5. 写入运行级结果并更新任务状态。

`verify` 检查 Requirement/Scenario、任务和证据覆盖；`review` 追加确定性审查、范围检查、敏感信息扫描与 Minimality Review。后者通过 Git 快照比较 `package.json` 依赖变化、文件/行数指标和变更文件内的 `sdd-debt` 标记。未在计划中声明的新增依赖返回 `E_UNPLANNED_DEPENDENCY` 并生成 REPAIR 任务；复杂度与合法债务只记录，不阻断归档。

## 制品与原子写入

所有运行事实写入 `.sdd/`，子目录按需创建：

- `.sdd/state.json`：工作流事实源。
- `.sdd/artifacts.json`：制品输入摘要和内容哈希的集中清单。
- `.sdd/changes/<change-id>/`：当前变更制品。
- `.sdd/context-packs/`：按任务生成的上下文。
- `.sdd/runs/`：运行级任务结果。
- `.sdd/loop/`：自动流程规格、事件和运行记录。
- `.sdd/index/`：代码库摘要和 MCP 诊断。

状态和成组制品使用临时文件、同步、重命名及备份恢复；不会再为每个制品生成 `.meta.json` sidecar。

## 归档

```text
规格 + 设计 + 计划 + 任务结果
  + verify/review 报告
  + Git 基线与快照
  + 追踪矩阵
  → archive.json
  → archive.md
  → .archived
```

`archive` 在同一写锁内重新验证报告摘要、Git 快照、文件范围、漂移和追踪闭环，并将简洁性指标、依赖 delta、债务与 Policy 来源写入归档。成功后删除展开制品，变更目录只保留三个归档文件。`.archived` 保存组合哈希；状态更新中断时，再次执行 `archive` 会根据有效标记收敛到 `ARCHIVED`。

## 代码库理解与降级

初始化时优先托管固定版本的 `codebase-memory-mcp`。查询失败或组件不可用时，Core 使用 `fallback-file-scan`，同时写入诊断并返回 warning；降级不会被静默隐藏。

仓库内容和 MCP 输出都按不可信数据处理，进入 Prompt 前必须包裹边界，不能覆盖系统约束或扩大任务权限。

## Git 隔离

启用 `workflow.gitIsolation` 后，`GitIsolationManager` 为变更创建或复用独立分支与 worktree。业务代码操作在 worktree 中执行，`.sdd/` 仍保留在控制根目录。系统不会自动 merge、push、reset、clean 或删除 worktree。

## 上游快照

`vendor/openspec/upstream/` 和 `vendor/superpowers/upstream/` 是固定版本的审计快照，不作为外部 CLI 执行。Ponytail 仅以固定提交的最小正确实现方法论改写为受控 Policy，不安装其 npm 包、Hook、MCP 或模式系统。运行时只复用受控规则的语义，流程编排、状态、安全和质量门禁仍由 sdd-harness 实现。发布校验使用 `VERSION.json`、`MANIFEST.sha256` 和许可证信息验证快照完整性。
