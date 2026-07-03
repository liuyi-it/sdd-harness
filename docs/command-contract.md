# Command Contract

Public commands are `init`, `auto`, `new`, `design`, `plan`, `build`, `verify`, `review`, `archive`, and `status`.

Common options are `--json`, `--non-interactive`, `--force`, `--timeout <seconds>`, `--change <id>`, and `--verbose`. Claude Code uses `/sdd.<command>`; Codex uses `sdd <command>`. Both adapters produce the same `CommandRequest` and return the same `CommandResult`.

`CommandResult` contains `ok`, `state`, `exitCode`, optional `changeId`, `next`, `data`, `warnings`, and structured `error`. Exit codes follow `需求文档.md`, including 124 for timeout and 130 for interruption.
