# 架构说明

## 分层与依赖

```text
Agent Adapter ──> sdd-cli (bin) ──> sdd-core (lib)
                                      ├── commands / state / quality / security
                                      ├── git / engines / protocol / policies
                                      └── knowledge（GitNexus / CodeGraph）
```

- `crates/sdd-cli`：clap 解析参数、路由命令并渲染 `CommandResult`。
- `crates/sdd-core`：唯一状态机与质量门禁执行层。
- `crates/sdd-core/src/protocol`：定义 Agent 行动要求、结果和约束结构。
- `crates/sdd-core/src/policies`：按阶段解析 Policy，并生成可校验摘要。
- `crates/sdd-core/src/knowledge`：GitNexus / CodeGraph 双引擎探测、索引、路由与降级。
- `assets/adapters/*`：把宿主指令翻译为 CLI 调用，不直接修改状态（编译期嵌入二进制）。

Core 是唯一推进状态机的入口，外部 Agent 或工具不能绕过 Core 推进阶段。

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
  → TddEngine / Superpowers planner
  → plan.json
  → build next
  → 单任务 Context Pack
```

`spec.json` 是规格事实源，包含 proposal、impact、澄清结果、delta 和 Requirement/Scenario 模型。`plan.json` 是计划事实源，包含任务定义、可读计划、测试计划和上下文摘要。

Context Pack 不在 `plan` 阶段批量生成。`build next` 只为当前任务创建 `.sdd/context-packs/<task-id>/context.md`，并根据规格、计划、源码和 Policy 摘要自动刷新。

## 构建与质量门禁

任务采用 RED、GREEN、REFACTOR、VERIFY 四阶段链。Core 对 Agent 结果执行以下裁决：

1. 验证 TaskExecutionResult 结构和任务身份。
2. 使用运行时任务状态（state.tasks）检查依赖完成情况。
3. 校验 TDD evidence（RED 失败证据 / GREEN 通过证据 / VERIFY 完整验证）。
4. 写入运行级结果并更新任务状态。

`verify` 检查 Requirement/Scenario、任务和证据覆盖；`review` 追加确定性审查、范围检查与敏感信息扫描。`archive` 在同一写锁内重新验证报告摘要，归档收敛为三个文件。

## 制品与原子写入

所有运行事实写入 `.sdd/`，子目录按需创建：

- `.sdd/state.json`：工作流事实源。
- `.sdd/artifacts.json`：制品输入摘要和内容哈希的集中清单。
- `.sdd/config.json`：项目配置（Rust 版格式，由 YAML 重构为 JSON）。
- `.sdd/changes/<change-id>/`：当前变更制品。
- `.sdd/context-packs/`：按任务生成的上下文。
- `.sdd/runs/`：运行级任务结果。
- `.sdd/index/`：代码库摘要和知识图谱诊断。

状态和成组制品使用临时文件、同步、重命名及备份恢复。

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

`archive` 在同一写锁内重新验证报告摘要，归档收敛为三个文件。`.archived` 保存组合哈希；状态更新中断时，再次执行 `archive` 会根据有效标记收敛到 `ARCHIVED`。

## 代码库理解与降级

初始化时自动探测 PATH 中的 `gitnexus` 与 `codegraph` 命令，对可用引擎各自索引（`.gitnexus/`、`.codegraph/` 落在业务项目根）。查询按 intent 路由（impact/context/related-files 等走 GitNexus 优先；explore/callers/callees 走 CodeGraph 优先），两级引擎均不可用或失败时使用 `fallback-file-scan` 受限文件扫描，同时写入诊断并返回 warning；降级不会被静默隐藏。

仓库内容和引擎输出都按不可信数据处理，进入 Prompt 前必须包裹边界，不能覆盖系统约束或扩大任务权限。

## Git 隔离

`workflow.gitIsolation`（config.json）启用后，变更在独立分支与 worktree 中执行。Rust 版一期未实现 worktree 隔离（默认关闭），后续版本接入。系统不会自动 merge、push、reset、clean 或删除 worktree。

## 上游快照

`vendor/openspec/` 和 `vendor/superpowers/` 是固定版本的审计快照，不作为外部 CLI 执行。运行时只复用受控规则的语义，流程编排、状态、安全和质量门禁仍由 sdd-harness 实现。
