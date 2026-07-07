# SDD Harness Integration

当用户请求 SDD 变更时，运行：

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
