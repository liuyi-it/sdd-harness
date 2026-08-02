# 命令与制品契约

公开工作流命令为 `init`、`status`、`new`、`design`、`plan`、`build`、`verify`、`review`、`archive`、`auto` 和 `codebase`。CLI、Adapter 与 Core API 最终都使用同一个 `CommandRequest` / `CommandResult` 契约。

## CommandResult

每次调用返回：

- `ok`、`state`、`exitCode`：必填结果字段。
- `changeId`、`next`、`data`：可选流程信息。
- `warnings`：降级或诊断信息。
- `actionRequired`：需要 Agent 执行任务时返回。
- `error`：稳定错误码、消息和建议命令。

CLI 进程退出码必须等于 `CommandResult.exitCode`。

## 规格与计划

- `new` 写入人工可读的 `spec.md` 和机器事实源 `spec.json`。
- `design` 写入 `design.md`。
- `plan` 只写入 `plan.json`，不批量创建 Context Pack；可选 `dependencies` 决策以 `name`、`manifest`、`action`、原因和 Requirement ID 记录必要的依赖变化。
- `build next` 为选中的任务按需生成 Context Pack，并返回 `AGENT_TASK_EXECUTION`。

新结构不读取旧的多文件规格/计划布局。

## AgentTaskExecution

`actionRequired` 至少包含任务 ID、变更 ID、Context Pack 路径、允许/期望新增/禁止文件、结构化 verification、结果文件路径、codebase 状态和可选 Policy Bundle。

TaskExecutionResult 必须带有任务状态、文件变化、命令证据和 TDD evidence：

- RED 至少包含一条 `passed=false`、`expectedFailure=true` 的证据。
- GREEN、REFACTOR、VERIFY 的阶段证据必须通过，且不能声明预期失败。
- VERIFY 必须提供最终 verification。
- 可选 `minimality` evidence 可说明复用、标准库/平台选择、依赖、抽象和有意债务；它只作审计辅助，Core 仍以 Git delta 与 manifest 为事实源。
- 实际文件范围以 Git delta 为事实源，Agent 声明不能扩大权限。

违反任务证据契约返回 `E_TDD_EVIDENCE_REQUIRED`；越权文件或命令返回相应安全错误。

## 验证、审查与修复

`verify` 读取 `spec.json` 和 `plan.json`，检查场景级任务与证据覆盖。`review` 在 verify 快照基础上执行确定性审查、敏感信息扫描和 Minimality Review：比较 Cargo 依赖名、统计变更文件，并扫描本次 delta 中显式的 `sdd-debt` / `ponytail:` 标记。

新增依赖必须在 `plan.json.dependencies` 中以 `ADD` 声明，否则返回 `E_UNPLANNED_DEPENDENCY`（Rust 版依赖事实源为 `Cargo.toml`）。改动规模和债务 finding 默认不阻断；安全、Spec、文件范围和 TDD 门禁优先级不变。

verify/review 失败会保留失败报告：证据或验证快照失效时回到 `BUILD_READY`，可直接重试的审查问题保留在 `VERIFY_READY`。

## 归档

`archive` 重新验证 PASS 报告、任务结果、Git 快照、漂移和追踪链，然后生成：

- `archive.json`：完整机器归档，含规格、设计、计划、任务结果、质量报告和 Git 摘要。
- `archive.md`：归档报告与质量摘要。
- `.archived`：`archive.md` 与 `archive.json` 的组合内容哈希。

Marker 最后发布。有效 marker 存在但状态尚未更新时，再次执行命令会收敛状态；无效或被篡改的 marker 会被拒绝。
