# 命令与制品契约

公开工作流命令为 `init`、`status`、`new`、`change`、`design`、`plan`、`build`、`verify`、`review`、`archive`、`auto` 和 `codebase`。CLI、Adapter 与 Core API 最终都使用同一个 `CommandRequest` / `CommandResult` 契约。

## CommandResult

每次调用返回：

- `ok`、`state`、`exitCode`：必填结果字段。
- `changeId`、`next`、`data`：可选流程信息。
- `warnings`：降级或诊断信息。
- `actionRequired`：需要 Agent 执行任务时返回。
- `error`：稳定错误码、消息和建议命令。

CLI 进程退出码必须等于 `CommandResult.exitCode`。


## auto 恢复契约

`auto` 的 `answers` 参数必须是 JSON 对象，与 `new --answers` 使用相同的键值格式。`sdd auto --resume --answers '<JSON>'` 会把答案透传给当前 `NEW_STARTED`/`CLARIFYING` 变更；不携带需求文本时从当前 `runId` 的 runtime 输入恢复。

`NEW_STARTED` 的 `CommandResult.next` 为 `sdd auto --resume`。若当前状态缺少 `changeId` 或 `runId`，Core 返回 `E_STATE_CORRUPTED` 并建议 `sdd status`，不创建新的恢复对象。

## 规格与计划

- `new` 写入人工审核规格 `spec.md`，机器规格模型写入 `.sdd/runtime.json` 的 `changes.<changeId>.spec`。
- `design` 写入机器设计到 `runtime.changes.<changeId>.design`，并生成人工审核 `design.md`。
- `build next` 为选中的任务按需生成内联 Context Pack，并返回 `AGENT_TASK_EXECUTION`。


## 需求修改契约

`change` 只接受当前活动且未归档的 `changeId`，并要求非空的新需求。成功时直接重写当前 `spec.md` 和 `proposal.md`，删除由旧需求生成的 `design.md`、`plan.md`、`tasks.md` 及其机器状态，工作流回到 `SPEC_READY`，`CommandResult.next` 为 `sdd design`。不生成需求级备份文件、修订目录或修订 ID；runtime 的崩溃恢复备份不属于需求历史，需求历史由 Git 提供。

文档写入失败时只使用进程内的旧内容恢复当前目录，不留下额外制品；拒绝事件仍记录稳定错误原因。

## AgentTaskExecution

`actionRequired` 至少包含任务 ID、变更 ID、完整 Context Pack 内容、允许/期望新增/禁止文件、结构化 verification、`resultTransport: "inline-json"`、codebase 状态和可选 Policy Bundle。

`build complete` 接受 `--result-json` 内联 JSON 或 `--result <path>` 文件；两者都解析为同一个 `TaskExecutionResult`，实际文件范围仍以 Git delta 为事实源。

TaskExecutionResult 必须带有任务状态、文件变化、命令证据和 TDD evidence：

- RED 至少包含一条 `passed=false`、`expectedFailure=true` 的证据。
- GREEN、REFACTOR、VERIFY 的阶段证据必须通过，且不能声明预期失败。
- VERIFY 必须提供最终 verification。
- 可选 `minimality` evidence 可说明复用、标准库/平台选择、依赖、抽象和有意债务；它只作审计辅助，Core 仍以 Git delta 与 manifest 为事实源。
- 实际文件范围以 Git delta 为事实源，Agent 声明不能扩大权限。

违反任务证据契约返回 `E_TDD_EVIDENCE_REQUIRED`；越权文件或命令返回相应安全错误。

## 验证、审查与修复

`verify` 读取 runtime 中的规格、计划和任务结果，检查场景级任务与证据覆盖。`review` 在 verify 快照基础上执行确定性审查、敏感信息扫描和 Minimality Review：比较 Cargo 依赖名、统计变更文件，并扫描本次 delta 中显式的 `sdd-debt` / `ponytail:` 标记。

新增依赖必须在 runtime 计划的 `dependencies` 中以 `ADD` 声明，否则返回 `E_UNPLANNED_DEPENDENCY`（Rust 版依赖事实源为 `Cargo.toml`）。改动规模和债务 finding 默认不阻断；安全、Spec、文件范围和 TDD 门禁优先级不变。

`review` 还支持可选的 OCR 后端（Alibaba Open Code Review），由 `quality.ocr.mode` 控制：`auto`（默认）在确定性审查通过且存在变更文件时尝试调用 `quality.ocr.command`（默认 `ocr`），找不到时仅返回 `W_OCR_NOT_FOUND` 警告并保留确定性结论；`off` 不启动；`required` 找不到时返回 `E_REVIEW_BACKEND_UNAVAILABLE`。确定性扫描未通过或存在安全/范围阻断时不启动 OCR。OCR 已启动后的超时、非零退出、失败状态或非法 JSON/finding 使用稳定错误码 `E_REVIEW_BACKEND_TIMEOUT`、`E_REVIEW_BACKEND_FAILED`、`E_REVIEW_BACKEND_INVALID_OUTPUT`、`E_REVIEW_BACKEND_UNAVAILABLE` 硬失败，并持久化 `passed=false` 报告、回到 `VERIFY_READY`。OCR 发现的每条 finding 以 `origin="ocr"`、`category`、`startLine`/`endLine`、`suggestionCode` 合并进报告；OCR 的 prompt、thinking、API key 与完整 stderr 不会被持久化。

verify/review 失败会保留失败报告：证据或验证快照失效时回到 `BUILD_READY`，可直接重试的审查问题保留在 `VERIFY_READY`。

## 归档

`archive` 重新验证 PASS 报告、任务结果、Git 快照、漂移和追踪链，然后生成：

- `runtime.changes.<changeId>.archive`：完整机器归档，含规格、设计、计划、任务结果、质量报告和 Git 摘要。
- `archive.md`：合并后的人工归档文档，包含规格、计划、任务、验证和审查结果。

归档完成后变更目录只保留 `archive.md`；其他制品、状态、配置、报告、loop、索引和机器归档均保留在 `.sdd/runtime.json`。`runtime.json` 通过临时文件、原子替换和 `runtime.json.bak` 恢复。
