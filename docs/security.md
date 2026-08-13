# 安全策略

- 所有路径都会被解析到真实仓库根目录之下；系统会阻断 POSIX 路径穿越、Windows 盘符路径、UNC 路径、反斜杠穿越、写入 `.git` 和指向仓库外的符号链接。
- 启用 `workflow.gitIsolation` 后，Git worktree 只通过 `std::process::Command` 的固定 argv 执行 `rev-parse/status/worktree`，不经 shell 拼接。
- `build` 结果必须通过任务允许文件、期望新增文件和禁止文件三类范围校验。
- 只有批准的本地构建与测试命令前缀可以作为验证证据；Git、Shell 操作符、网络命令和破坏性命令都会被拒绝。
- 仓库内容和 CodeGraph 输出只被当作不可信数据，不会扩展任务范围或验证命令权限。
- `build next` 会验证计划中的 verification 前缀并拒绝 shell 元字符、网络入口和破坏性命令；Agent 返回的命令必须与计划完全一致。
- `review` 会重新扫描 Git 真实变更文件；命中 token、私钥、JWT、Authorization 或数据库密码等规则时阻断归档，报告不保留原值。
- worktree 复用时若发现基线漂移、注册路径不一致、路径被占用或存在脏改动，会直接阻断；系统不会自动 `reset`、`clean`、`merge`、`push` 或删除 worktree。
- `review-report.json` / `verify-report.json` 是质量闸门的机器可读事实源；即使阶段失败也先落盘，避免“失败但无报告”。
- CodeGraph 引擎输出按不可信数据处理，进入 Prompt 前必须包裹边界。
