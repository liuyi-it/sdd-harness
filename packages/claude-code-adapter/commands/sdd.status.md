执行以下 CLI 命令并将结果展示给用户：

```bash
sdd status --json
```

如果用户指定了路径，使用 `--cwd <path>`。
如果 JSON 输出包含 `warnings`，高亮提示用户。
如果 `codebase.degraded` 为 `true`，建议执行 `sdd codebase doctor`。
