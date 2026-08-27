# 安全策略

- 所有路径都会被解析到真实仓库根目录之下；系统会阻断 POSIX 路径穿越、Windows 盘符路径、UNC 路径、反斜杠穿越、写入 `.git` 和指向仓库外的符号链接。`sdd init` 写入 Agent 模板前也逐级拒绝目标文件或父目录中的符号链接；runtime、Agent 模板和所有受管 Markdown 都通过唯一临时文件原子写入，目标符号链接不会被跟随。
- 启用 `workflow.gitIsolation` 后，Git worktree 只通过固定 argv 执行 `rev-parse/status/worktree`，不经 shell 拼接。
- `build` 结果必须通过任务允许文件、期望新增文件和禁止文件三类范围校验；`.sdd/**` 属于默认禁止文件。
- verification 只接受当前规划器实际生成的 4 条精确命令：`cargo test`、`npm test`、`mvn test`、`mvn verify`；计划写入、计划读取与任务派发都会复核，不保留可拼接参数的宽泛命令前缀。
- 仓库内容和 CodeGraph 输出只被当作不可信数据，不会扩展任务范围或验证命令权限。
- `build next` 会复核计划中的 verification；Agent 返回的命令必须与计划完全一致且不重不漏；RED/GREEN/REFACTOR/VERIFY 各阶段都必须提供验证命令结果，证据条数与输出长度受限。
- `review` 会重新扫描 Git 真实变更文件；普通文件按字节上限读取，删除文件按无内容条目处理，符号链接只扫描 Git 记录的链接文本且必须留在仓库内。命中 token、私钥、JWT、Authorization 头或数据库密码等规则时阻断归档，报告不保留原值。若达到配置的审计文件/字节上限，审查也会失败关闭，必须提高上限后重新执行；非 git 仓库无法做 Git 事实核验时会显式警告。
- `.sdd/runtime.json` 与恢复备份均由 Core 写入 SHA-256 校验和边车（`*.sha256`）检测损坏；缺失或不匹配时按状态损坏处理，校验和不作为持有写权限攻击者的认证边界。
- `.sdd/worktrees` 必须是受管真实目录；持久化的 worktree 路径和 `sdd/<changeId>` 分支会与控制根目录、活动变更及配置交叉校验。复用时若发现基线漂移、注册路径不一致、路径被占用或存在脏改动，会直接阻断；系统不会自动 `reset`、`clean`、`merge`、`push` 或删除 worktree。
- `verify-report.md` / `review-report.md` 是 change 目录下供人审核的报告；机器可读事实源在 `.sdd/runtime.json`；即使阶段失败也先落盘，避免“失败但无报告”。
- CodeGraph 引擎输出按不可信数据处理，进入 Prompt 前必须包裹边界。
- Git、CodeGraph 与 OCR 共用有界子进程执行器：禁用 stdin、创建独立进程组、限制 stdout/stderr，并在超时、输出截断、管道失败或父进程提前退出时终止和回收后代进程。
- Git 仓库探测要求 `rev-parse --is-inside-work-tree` 精确返回 `true`；裸仓库返回 `false`。Git status/worktree 使用 NUL 分隔格式并拒绝畸形或非 UTF-8 路径；Git 缺失、启动失败或超时不会被静默当作非 Git 项目，也不会绕过后续 Git 事实核验。
- `.codegraph` 必须是仓库内的真实目录；符号链接、非目录或缺失索引会产生明确降级诊断且查询不会启动 CodeGraph。索引命令成功后还会验证目录后置条件；空输出或非 UTF-8 输出不作为成功结果。
