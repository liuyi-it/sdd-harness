# 安全

## 状态与路径

- `.sdd/runtime.json` 是唯一机器事实源，主文件和备份分别使用 SHA-256 检测损坏。
- Runtime 版本不匹配直接拒绝，不尝试迁移或读取旧结构。
- 受管文件使用原子写入；所有输入路径拒绝绝对路径、父目录穿越、`.git` / `.sdd` 业务修改和符号链接逃逸。
- Git 隔离路径必须与 `.sdd/worktrees/<change-id>`、`sdd/<change-id>` 分支和控制仓库一致。

## Agent 边界

代码库摘要、Context Pack 和仓库文件都是不可信上下文，不能当作指令。阶段 JSON 必须通过对应 Schema，并拒绝 TODO、TBD、待补充等占位内容。

`AGENT_TASK_EXECUTION` 限定 allowedFiles、expectedNewFiles、forbiddenFiles 与 verification。Git 仓库中，Agent 声明的 filesChanged 必须与派发时基线后的真实 delta 完全一致；非 Git 项目显式警告事实边界。

验证命令不经过 shell，且会拒绝解释器、命令替换、重定向、管道和不在允许集合内的程序。任务和修复结果的 inline JSON 上限为 4 MiB，各证据字段另有限额。

## 统一质量门禁

`verify` 在一个报告中检查：

- Requirement/Scenario 是否由 DONE 任务和有效证据覆盖；
- 实际变更是否在全部计划任务的允许范围内；
- Cargo.toml 新增依赖是否在 plan.dependencies 中声明为 ADD；
- 变更文件是否包含 token、私钥、JWT、Authorization 或密码等敏感模式；
- 审计文件数和字节数是否超过配置上限，超过时失败关闭；
- 质量报告后的 Git 工作区指纹是否在归档前保持不变。

报告不保存检测到的秘密原值。自动修复只能修改计划允许文件并执行全部计划 verification；默认一轮，防止无限修复循环。

## 不执行的外部操作

Core 不自动 commit、merge、push、发布、删除 worktree 或调用远端模型。CodeGraph 是 PATH 中的可选本地 CLI；不可用时只做受限本地文件扫描。
