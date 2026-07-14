# 命令契约

公开命令包括 `init`、`auto`、`new`、`design`、`plan`、`build`、`verify`、`review`、`archive` 与 `status`。

通用参数包括 `--json`、`--non-interactive`、`--force`、`--timeout <seconds>`、`--change <id>` 和 `--verbose`。Claude Code 使用 `/sdd.<command>`，Codex 使用 `sdd <command>`。两个适配器都会生成相同的 `CommandRequest`，并返回相同结构的 `CommandResult`。

`CommandResult` 包含 `ok`、`state`、`exitCode`，以及可选的 `changeId`、`next`、`data`、`warnings` 和结构化 `error`。退出码定义遵循 `需求文档.md`，其中 `124` 表示超时，`130` 表示用户中断。

`new` 成功后生成供人工阅读的 `spec.md` 和机器事实源 `spec.json`；后者集中保存 proposal、impact、澄清信息、delta 与 Requirement/Scenario 模型。`plan` 只生成 `plan.json`，其中每个任务包含 `phase`、Requirement、Scenario、依赖、精确文件范围和验证命令，并同时保存可读计划、测试计划与上下文摘要。

`TaskExecutor` 返回值必须包含 `tddEvidence`。RED 阶段至少有一条 `passed=false` 且 `expectedFailure=true` 的证据；GREEN、REFACTOR、VERIFY 必须通过且不得声明预期失败；VERIFY 还必须提供最终 `verification`。违反协议返回 `E_TDD_EVIDENCE_REQUIRED`（退出码 `7`）。

`verify` 读取 `spec.json` 作为规格事实源，并检查场景级任务与证据覆盖。`archive` 不仅检查已有 PASS 报告，还会重新验证报告摘要、任务结果、Git 快照、漂移和追踪链，然后把机器数据压缩到 `archive.json`、把归档报告与追踪矩阵合并到 `archive.md`，最终仅保留这两个文件和 `.archived`。
