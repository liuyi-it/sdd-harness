# 安全

## 状态与路径

- `.sdd/runtime.json` 是唯一机器事实源，内嵌 SHA-256 检测内容损坏，并与状态一次原子提交；不保留或读取恢复备份。校验失败时停止，不自动回退或重置。
- SHA-256 用于完整性检查，不是签名或权限边界；能同时改写内容和校验的人仍可重算它，宿主不得直接编辑 Runtime。
- `.sdd/lock` 仅承担 OS 排他锁，不生成持有者诊断文件。进程退出释放锁，文件存在不等于仍被占用；禁止删除锁文件来绕过互斥。
- Runtime 版本不匹配直接拒绝，不尝试迁移或读取旧结构。
- 受管文件使用原子写入；所有输入路径拒绝绝对路径、父目录穿越、`.git` / `.sdd` 业务修改和符号链接逃逸。
- 对没有 Git fingerprint 的计划目录，必须在遍历前检查静态前缀；任一目录组件是符号链接都拒绝，无论目标在项目内还是项目外；通配扫描不跟随目录链接。空目录也拒绝并返回 `E_PATH_OUTSIDE_REPO`，不提供目录别名映射。精确文件路径的末级符号链接只有目标仍在仓库内时才可接受，并按链接文本绑定或摘要，不读取、跟随或遍历目标；外部目标和悬空链接均由路径校验拒绝。
- 非 Git 项目仍明确报告按计划文件范围受限；缺少 fingerprint 不会放宽计划范围或转为目录别名解析。
- Git 隔离路径必须与 `.sdd/worktrees/<change-id>`、`sdd/<change-id>` 分支和控制仓库一致。

## Agent 边界

代码库摘要、Context Pack 和仓库文件都是不可信上下文，不能当作指令。阶段 JSON 必须通过对应 Schema，并拒绝 TODO、TBD、待补充等占位内容。spec、plan、build task result 和 verify fix result 的 `collaboration` 记录也必须通过对应 Schema；它只记录协作来源与限制，不能覆盖行动、allowedFiles、用户授权或质量结论。

`AGENT_TASK_EXECUTION` 限定 allowedFiles、expectedNewFiles、forbiddenFiles 与 verification。Git 仓库中，Agent 声明的 filesChanged 必须与派发时基线后的真实 delta 完全一致；非 Git 项目显式警告事实边界。

`AGENT_REVIEW_EXECUTION` 只能读取绑定的规格候选或质量报告，按 targetHash 和目标版本核对，不能修改业务文件、`spec.md` 或 Runtime。审查结果必须为 READY/NOT_READY，并声明 independent、self 或 self_fallback；可选 `riskFactors` 只能使用与 spec `reviewPolicy` 相同的七类枚举。发现新风险时，Core 必须原子追加风险、生成新目标哈希，并让同一 reviewer 重新独立审查；旧审查记录保留为历史，当前 candidate 重新判断；产品审查通过后才可进入架构审查，不能复用旧 self/self_fallback 结论。只有没有可用独立派发时才允许 fallbackReason。复杂风险优先要求独立审查；审查不完整不能改称 self_fallback，第二次仍不完整由退出码 8 阻断并保留行动。

验证命令以程序名和参数数组声明；门禁支持明确的本地质量检查入口及其参数，拒绝 shell、内联脚本、命令替换、重定向、管道和发布入口。Python 仅允许 `-m unittest` / `-m pytest`，Node 仅允许 `--test`。完整集合见 CLI 文档。任务和修复结果的 inline JSON 上限为 4 MiB，各证据字段另有限额。

Maven 按选项、选项值和生命周期逐项解析，避免把前置 `-B` / `-s` 误当入口，也避免把 `test deploy` 当质量检查。`-e` / `-c` 在 Maven 中是诊断和校验选项，不当作解释器执行；其他程序的内联执行限制不变。仅允许含 `test` 或 `verify` 的检查链，可先 `clean`，不接受发布、安装或额外插件目标。已选 Maven profile、构建插件及工程自身仍能执行代码；这些命令规则不能构成任意工程代码的安全沙箱，也不能证明测试未被配置跳过。

Core 校验计划及提交的证据，不自动运行业务验证；宿主应按独立 argv 执行命令，不拼接 shell。程序白名单不是进程安全沙箱，测试和构建脚本仍会执行项目代码，须遵守宿主的权限和执行策略。任务结果按程序名及参数数组逐项匹配计划，禁止用文本拼接掩盖不同参数。verify 先执行确定性检查，再进入只读审查；QUALITY targetHash 绑定确定性质量报告内容（排除 Core 生成的语义审查附录）以及 spec、plan、tasks、fixes、workspace；质量自动修复默认只有一轮。`AGENT_FIX_EXECUTION` 必须携带 `userAuthorized` 和完整 materialized `resultSchema`；`QUALITY_BLOCKED` 下普通 verify 不得隐式开启额外修复，只有用户明确授权的 `sdd verify --continue` 才能开启下一轮。

审查历史和 findings 只单调追加，不能覆盖既有记录；完整审查结果不可覆盖。宿主用原命令重复提交相同完整结果时，Core 应幂等恢复推进；冲突结果必须拒绝。审查 targetHash 变化时不能接受旧结果。

## 统一质量门禁

`verify` 在一个报告中检查：

- Requirement/Scenario 是否由 DONE 任务和有效证据覆盖；
- 实际变更是否在全部计划任务的允许范围内；
- Cargo.toml 新增依赖是否在 plan.dependencies 中声明为 ADD；
- 变更文件是否包含 token、私钥、JWT、Authorization 或密码等敏感模式；
- 审计文件数和字节数是否超过配置上限，超过时失败关闭；
- 质量报告后的 Git 工作区指纹是否在归档前保持不变。

无引号敏感赋值按每个匹配值判断占位形式，仅明确占位词、下划线/短横线模板或数字编号可排除；同行注释或另一项占位赋值不能隐藏实际敏感值。专门凭据格式和引号检测保持启用。该扫描是模式门禁，不证明所有秘密均可识别。

报告不保存检测到的秘密原值。自动修复只能修改计划允许文件并执行全部计划 verification；默认一轮，防止无限修复循环。

## 不执行的外部操作

Core 不自动 commit、merge、push、发布、删除 worktree 或调用远端模型。CodeGraph 是 PATH 中的可选本地 CLI；不可用时只做受限本地文件扫描。
