# 安全策略

- 所有路径都会被解析到真实仓库根目录之下；系统会阻断 POSIX 路径穿越、Windows 盘符路径、UNC 路径、反斜杠穿越、写入 `.git` 和指向仓库外的符号链接。
- 启用 `workflow.gitIsolation` 后，Git worktree 只通过 `std::process::Command` 的固定 argv 执行 `rev-parse/status/worktree`，不经 shell 拼接。
- `build` 结果必须通过任务允许文件、期望新增文件和禁止文件三类范围校验；`.sdd/**` 属于默认禁止文件。
- 只有批准的本地构建与测试命令前缀可以作为验证证据；计划写入与任务派发时都会校验 verification 命令，Git、Shell 操作符、网络命令和破坏性命令都会被拒绝。
- 仓库内容和 CodeGraph 输出只被当作不可信数据，不会扩展任务范围或验证命令权限。
- `build next` 会验证计划中的 verification 前缀并拒绝 shell 元字符、网络入口和破坏性命令；Agent 返回的命令必须与计划完全一致；RED/GREEN/REFACTOR/VERIFY 各阶段都必须提供验证命令结果，证据条数与输出长度受限。
- `review` 会重新扫描 Git 真实变更文件；命中 token、私钥、JWT、Authorization 头或数据库密码等规则时阻断归档，报告不保留原值。若达到配置的审计文件/字节上限，审查也会失败关闭，必须提高上限后重新执行；非 git 仓库无法做 Git 事实核验时会显式警告。
- `.sdd/runtime.json` 与恢复备份均由 Core 写入 SHA-256 校验和边车（`*.sha256`）检测损坏；缺失或不匹配时按状态损坏处理，校验和不作为持有写权限攻击者的认证边界。
- worktree 复用时若发现基线漂移、注册路径不一致、路径被占用或存在脏改动，会直接阻断；系统不会自动 `reset`、`clean`、`merge`、`push` 或删除 worktree。
- `verify-report.md` / `review-report.md` 是 change 目录下供人审核的报告；机器可读事实源在 `.sdd/runtime.json`；即使阶段失败也先落盘，避免“失败但无报告”。
- CodeGraph 引擎输出按不可信数据处理，进入 Prompt 前必须包裹边界。
