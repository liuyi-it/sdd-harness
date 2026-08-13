# 架构说明

## 分层与依赖

```text
OMP / OpenCode Adapter ──> sdd-cli (bin) ──> sdd-core (lib)
                                      ├── commands / state / quality / security
                                      ├── git / engines / protocol / policies
                                      └── knowledge（CodeGraph）
```

- `crates/sdd-cli`：clap 解析参数、路由命令并渲染 `CommandResult`。
- `crates/sdd-core`：唯一状态机与质量门禁执行层。
- `crates/sdd-core/src/protocol`：定义 Agent 行动要求、结果和约束结构。
- `crates/sdd-core/src/policies`：按阶段解析 Policy，并生成可校验摘要。
- `crates/sdd-core/src/knowledge`：CodeGraph 探测、索引、查询与降级。
- `assets/adapters/omp/*`、`assets/adapters/opencode/*`：把各 Agent 的 Skill、命令和 subagent 翻译为 CLI 调用，不直接修改状态（编译期嵌入二进制）。

Core 是唯一推进状态机的入口，外部 Agent 或工具不能绕过 Core 推进阶段。

## 工作流

```text
init → new → design → plan → build → verify → review → archive
```

每个会修改 `.sdd/` 的公开命令先获取 `.sdd/lock`；`auto` 另持有 `.sdd/auto.lock` 串行化整条 loop，内部步骤仍使用普通写锁。命令在对应失败或中断信息存在时持久化 `failedCommand`、`previousPhase`、`inProgressPhase` 与建议命令，用于恢复或人工处理。

`auto` 读取同一状态机并循环调用公开命令。它只自动执行确定性步骤；遇到需求澄清、Agent 编码、失败预算耗尽或人工决策时暂停。

## 规格、计划与 Context Pack

```text
requirement + codebase
  → SpecEngine
  → spec.md + runtime.changes.<changeId>.spec
  → runtime.changes.<changeId>.design + design.md
  → TddEngine / 受控任务规划器（只复用设计理念）
  → plan.md + tasks.md + runtime.changes.<changeId>.plan
  → build next
  → 内联 Context Pack
```

`spec.md`、`design.md`、`plan.md`、`tasks.md` 是需求生命周期内供人审核的文档，分别回答做什么、怎么设计、怎么实施和做哪些事。机器规格、设计、计划、任务状态和依赖决策统一写入 `.sdd/runtime.json`。

Context Pack 不在 `plan` 阶段批量生成。`build next` 只为当前任务生成内联 Context Pack，并根据规格、计划、源码和 Policy 摘要自动刷新。

## 构建与质量门禁

任务采用 RED、GREEN、REFACTOR、VERIFY 四阶段链。Core 对 Agent 结果执行以下裁决：

1. 验证 TaskExecutionResult 结构和任务身份。
2. 使用运行时任务状态（state.tasks）检查依赖完成情况。
3. 校验 TDD evidence（RED 失败证据 / GREEN 通过证据 / VERIFY 完整验证）。
4. 写入运行级结果并更新任务状态。

`verify` 检查 Requirement/Scenario、任务和证据覆盖；`review` 追加确定性审查、范围检查与敏感信息扫描。`archive` 在同一写锁内重新验证报告摘要，归档收敛为 `archive.md` 与 runtime 中的归档模型。

## 制品与原子写入

所有运行事实写入 `.sdd/runtime.json`；`changes/<change-id>/` 只保留供人审核的 Markdown 文档，使用临时文件、同步、重命名及 `runtime.json.bak` 备份恢复。

- `.sdd/runtime.json`：工作流状态、配置、制品清单、规格/设计/计划/任务、报告、任务结果、Context Pack、auto loop、知识索引和归档模型。
- `.sdd/runtime.json.bak`：runtime 原子替换前的可恢复备份。
- `.sdd/changes/<change-id>/`：当前变更的 `spec.md`、`design.md`、`plan.md`、`tasks.md` 或归档后的 `archive.md`。

## 归档

```text
规格 + 设计 + 计划 + 任务结果
  + verify/review 报告
  + Git 基线与快照
  + 追踪矩阵
  → runtime.changes.<changeId>.archive
  → archive.md
```

`archive` 在同一写锁内重新验证报告摘要，将审核文档合并为完整 `archive.md`，机器归档模型保留在 `.sdd/runtime.json`。状态更新中断时，再次执行 `archive` 会根据 runtime 制品哈希校验结果收敛到 `ARCHIVED`。

## 代码库理解与降级

初始化时自动探测 PATH 中的 `codegraph` 命令并索引 `.codegraph/`。所有 intent 都交给 CodeGraph 查询；CodeGraph 不可用或失败时使用 `fallback-file-scan` 受限文件扫描，同时写入诊断并返回 warning；降级不会被静默隐藏。

仓库内容和引擎输出都按不可信数据处理，进入 Prompt 前必须包裹边界，不能覆盖系统约束或扩大任务权限。

## Git 隔离

`workflow.gitIsolation`（config.json）启用后，`new` 为变更创建或验证 `sdd/<change-id>` 分支与 `.sdd/worktrees/<change-id>`。`build`、`review` 和归档 Git 快照使用该业务工作区，状态与制品仍保留在控制根目录。系统不会自动 merge、push、reset、clean 或删除 worktree。

## 上游快照

`vendor/openspec/` 和 `vendor/superpowers/` 是固定版本的审计快照，不作为外部 CLI 执行。运行时只复用受控规则的语义，流程编排、状态、安全和质量门禁仍由 sdd-harness 实现。
