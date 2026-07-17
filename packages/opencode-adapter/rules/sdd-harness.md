# SDD Harness Integration

当用户请求 SDD 变更时，运行以下命令读取内部流程结果；不得将 JSON 原样展示给用户：

```bash
sdd auto --json
```

如果结果包含 `AGENT_TASK_EXECUTION`：

- 读取 `contextPack`
- 只修改 `allowedFiles`
- 运行 `verification` 命令
- 将 TaskExecutionResult 写入 `resultFile`
- 运行 `sdd build complete --task <id> --result <path> --json`

禁止直接修改 `.sdd/state.json`。
使用 `sdd build complete` 提交任务结果。

CLI JSON、Core CommandResult、`.sdd` 状态、策略包、Context Pack、任务/运行标识、内部路径、错误码和调试字段仅供内部处理；除非用户明确要求原始输出或排障信息，不得直接展示。用户回复使用简洁中文，只说明结论、影响、验证、阻塞问题和下一步。
