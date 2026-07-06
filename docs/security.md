# 安全策略

- 所有路径都会被解析到真实仓库根目录之下；系统会阻断 POSIX 路径穿越、Windows 盘符路径、UNC 路径、反斜杠穿越、写入 `.git` 和指向仓库外的符号链接。
- `build` 结果必须通过任务允许文件、期望新增文件和禁止文件三类范围校验。
- 只有批准的只读 Git 命令前缀和测试命令前缀可以作为验证证据；Shell 操作符、网络命令和破坏性命令都会被拒绝。
- 仓库内容和 MCP 输出只被当作数据，不会被当成指令执行；`impact.md`、Context Pack 和 MCP 摘要都必须显式包裹在 `UNTRUSTED_*` 边界中，拒绝伪造结束标记。
- TaskExecutor 只接收结构化 `constraints`；`allowedCommands` 会在 `build` 前去重、裁剪并过滤含 shell 元字符的命令，不能由不可信上下文扩权。
- 审计日志会对 token、密码、secret、API key 和授权字段做脱敏，并按配置大小轮转。
- `review` 会重新扫描 current-run diff 中的真实文件内容；命中 GitHub token、私钥、JWT、Authorization、数据库密码等规则时生成 `SECRET_LEAK`，阻断归档且报告中不保留原值。
- `review-report.v1.2.json` / `verify-report.v1.2.json` 是质量闸门的机器可读事实源；即使阶段失败也必须先落盘，避免“失败但无报告”。
- `codebase-memory-mcp` 的版本工件需要和固定校验清单进行完整性校验。
