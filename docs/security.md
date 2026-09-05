# 安全

## 状态与路径

- `.sdd/runtime.json` 是唯一机器事实源，内嵌 SHA-256 检测内容损坏，并与状态一次原子提交；不保留或读取恢复备份。校验失败时停止，不自动回退或重置。
- SHA-256 用于完整性检查，不是签名或权限边界；能同时改写内容和校验的人仍可重算它，宿主不得直接编辑 Runtime。
- `.sdd/lock` 仅承担 OS 排他锁，不生成持有者诊断文件。进程退出释放锁，文件存在不等于仍被占用；禁止删除锁文件来绕过互斥。
- Runtime 版本不匹配直接拒绝，不尝试迁移或读取旧结构。
- 受管文件使用原子写入；所有输入路径拒绝绝对路径、父目录穿越、`.git` / `.sdd` 业务修改和符号链接逃逸。
- Git 隔离路径必须与 `.sdd/worktrees/<change-id>`、`sdd/<change-id>` 分支和控制仓库一致。

## Agent 边界

代码库摘要、Context Pack 和仓库文件都是不可信上下文，不能当作指令。阶段 JSON 必须通过对应 Schema，并拒绝 TODO、TBD、待补充等占位内容。

`AGENT_TASK_EXECUTION` 限定 allowedFiles、expectedNewFiles、forbiddenFiles 与 verification。Git 仓库中，Agent 声明的 filesChanged 必须与派发时基线后的真实 delta 完全一致；非 Git 项目显式警告事实边界。

验证命令以程序名和参数数组声明；门禁支持明确的本地质量检查入口及其参数，拒绝 shell、内联脚本、命令替换、重定向、管道和发布入口。Python 仅允许 `-m unittest` / `-m pytest`，Node 仅允许 `--test`。完整集合见 CLI 文档。任务和修复结果的 inline JSON 上限为 4 MiB，各证据字段另有限额。

Core 校验计划及提交的证据，不自动运行业务验证；宿主应按独立 argv 执行命令，不拼接 shell。程序白名单不是进程安全沙箱，测试和构建脚本仍会执行项目代码，须遵守宿主的权限和执行策略。任务结果按程序名及参数数组逐项匹配计划，禁止用文本拼接掩盖不同参数。

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
