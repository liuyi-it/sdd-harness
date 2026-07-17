执行以下 CLI 命令读取内部状态；不得将 JSON 原样展示给用户：

```bash
sdd status --json
```

如果用户指定了路径，使用 `--cwd <path>`。
用简洁中文概述当前进度、正在处理的变更和用户可执行的下一步。
如果 JSON 输出包含 `warnings`，用业务可理解的语言提示用户。
如果 `codebase.degraded` 为 `true`，说明代码库索引能力受限，并建议执行 `sdd codebase doctor`。
除非用户明确要求原始输出或排障信息，不得展示 Core CommandResult、`.sdd` 状态、策略包、内部路径、运行标识、错误码或调试字段。
