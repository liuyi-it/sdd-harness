# 命令契约

公开命令包括 `init`、`auto`、`new`、`design`、`plan`、`build`、`verify`、`review`、`archive` 与 `status`。

通用参数包括 `--json`、`--non-interactive`、`--force`、`--timeout <seconds>`、`--change <id>` 和 `--verbose`。Claude Code 使用 `/sdd.<command>`，Codex 使用 `sdd <command>`。两个适配器都会生成相同的 `CommandRequest`，并返回相同结构的 `CommandResult`。

`CommandResult` 包含 `ok`、`state`、`exitCode`，以及可选的 `changeId`、`next`、`data`、`warnings` 和结构化 `error`。退出码定义遵循 `需求文档.md`，其中 `124` 表示超时，`130` 表示用户中断。
