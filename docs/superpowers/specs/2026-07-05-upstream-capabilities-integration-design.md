# OpenSpec 与 Superpowers 上游能力内置设计

## 目标与范围

本次以 `docs/需求文档.md` 的 MVP0–MVP2 为验收范围，修复当前 SpecEngine 与 TddEngine 仅生成通用模板、缺少上游核心语义的问题。MVP3 仅记录为后续增强，不纳入本次实现。

固定上游版本如下：

- OpenSpec `v1.4.1`，commit `1b06fddd59d8e592d5b5794a1970b22867e85b1f`
- Superpowers `v6.1.1`，commit `d884ae04edebef577e82ff7c4e143debd0bbec99`

## 架构

将两个固定提交的完整源码快照分别保存到 `vendor/openspec/upstream/` 与 `vendor/superpowers/upstream/`，作为可审计、不可自动漂移的上游基线。保留 LICENSE、版权、来源、版本、完整 commit 和本地修改说明。

上游 CLI、宿主安装器和默认事实源目录不直接暴露。`@sdd-harness/core` 增加内部适配层：SpecEngine 复用 OpenSpec 的 Requirement、Scenario、Delta、Validation 和 Archive 语义；TddEngine 将 Superpowers 的 brainstorming、writing-plans、test-driven-development 和 verification-before-completion 工作流转换为确定性阶段规则。Claude Code 与 Codex Adapter 仍只调用 Core。

所有最终状态和制品继续写入 `.sdd/`。vendor 目录是实现来源与许可证证据，不是运行时事实源。

## 行为与数据流

`sdd new` 将需求和代码库上下文转换为结构化 Requirement、Scenario 以及 ADDED/MODIFIED/REMOVED delta，并在生成制品前执行格式与一致性校验。无法确定验收行为时生成 BLOCKER questions，不得用通用断言代替用户意图。

`sdd design` 基于已验证规格和真实代码结构生成模块、接口、数据、事务、安全、错误处理与回滚设计。`sdd plan` 按 Requirement/Scenario 拆分原子任务，生成依赖关系、精确文件范围、实际项目验证命令及 Context Pack。

`sdd build` 对实现任务强制执行 RED、GREEN、REFACTOR、VERIFY 证据协议。`sdd verify`、`sdd review` 和 `sdd archive` 校验 Requirement → Scenario → Task → Test → Result 的完整追踪链；缺少任一环节不得推进状态或归档。

## 错误与恢复

规格格式错误、delta 冲突、任务依赖环、文件范围无效、缺少 TDD 证据、vendor 版本漂移和适配器不兼容均返回稳定错误码，并通过现有 FAILED/PAUSED 恢复模型记录 `previousPhase`、`failedCommand`、`failedReason` 和 `suggestedCommand`。不得静默降级为通用模板。

人工修改保护继续使用 metadata hash 与 candidate 文件；上游适配失败不得覆盖已有制品。

## 测试与发布验证

新增以下证据层：

1. OpenSpec 契约测试：Requirement/Scenario 解析、delta 合并与冲突、格式校验、归档映射。
2. Superpowers 契约测试：设计批准门、任务粒度、依赖排序和 RED→GREEN→REFACTOR→VERIFY 证据约束。
3. Core 集成测试：`.sdd/` 映射、candidate 防覆盖、失败恢复和 traceability 闭环。
4. Adapter 契约测试：Claude Code 与 Codex 的请求、状态、错误和制品一致。
5. E2E：粗略需求暂停澄清、完整需求自动归档、缺少测试证据时阻止推进。
6. CI：macOS × Windows、Claude Code × Codex 的组合验收，Node.js 20 为最低基线。

发布验证必须检查 vendor 固定 commit、文件清单、许可证、第三方声明和本地修改记录。最终执行 formatter、linter、typecheck、全量测试、Schema 校验、release 校验，并按需求文档逐条更新可验证的追踪矩阵。

## 非目标

- 不发布 OpenSpec 或 Superpowers 的独立 CLI。
- 不采用上游默认 `openspec/` 目录作为事实源。
- 不自动跟随上游版本。
- 不实现需求文档中的 MVP3 增强项。
