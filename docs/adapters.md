# Agent 接入

## Codex

`sdd init` 默认写入 `.agents/skills/`：

- `sdd-spec`：复用已有决策，以选择题和开放问题持续多轮澄清关键歧义，再生成规格与技术设计；
- `sdd-plan`、`sdd-build`、`sdd-verify`、`sdd-archive`：计划、实施、质量门禁和归档的阶段入口。

Codex 新初始化只写入这五个 Skill。`init`、`status`、`change` 和 `codebase` 仍是 CLI 命令，但不再各自占用 Skill。本项目不再安装专用 subagent 配置；是否使用通用 subagent 由宿主和用户决定，小任务不会被强制拆给多个角色。

每个阶段目录包含 `SKILL.md`、`references/workspace.md` 和 `references/collaboration.md`；后两者分别统一来自 `assets/policies/workspace.md` 与 `assets/policies/collaboration.md`，不是额外 Skill。所有入口执行命令前都须读取这两个本地参考。源模板的相对参考链接按安装布局解析，不能只复制 `SKILL.md` 而漏掉参考文件。

## OMP

OMP 宿主运行 `sdd init --host-adapter omp`，写入：

- `.omp/skills/` 下与 Codex 同名的五个 Skill；
- `.omp/commands/sdd.md` 自然语言入口；
- `/sdd.init`、`/sdd.status`、`/sdd.spec`、`/sdd.change`、`/sdd.plan`、`/sdd.build`、`/sdd.verify`、`/sdd.archive`、`/sdd.codebase` 全部显式命令。

OMP slash command 是快捷入口，不计入 Skill 数量。`/sdd` 会进入 `sdd-spec`；`/sdd.change` 先运行 `sdd change` 再进入同一统一规格阶段。当前只分发以上十个入口，已退役的 `new`、`design` 快捷入口已删除。重复初始化只刷新当前清单并保留其他既有文件，包括自定义文件；不自动扫描、删除或迁移旧项目资产。清理旧资产需先确认具体路径、来源与用户授权。

终端默认 Codex，OMP 自己传入隐藏宿主标识；用户无需在 `sdd init` 时选择。两端都分发同一份协作策略，Core 也将同一编译期内容放入适用行动上下文。

## AI-DLC 协作

协作参考位于 [collaboration.md](../assets/policies/collaboration.md)，角色职责以其中明确的六角色段为唯一来源；Core 的 `role_reference` 接口按角色名从该段提取职责。策略由 `assets.rs` 编译嵌入为 `COLLABORATION_POLICY`，并随两端十个阶段 Skill 的 `references/collaboration.md` 下发。六个角色是 `product`、`architect`、`developer`、`quality`、`product-reviewer`、`architecture-reviewer`，属于可组合的宽职责视角，不是新 Skill，也不是宿主必须安装的专用 subagent。

阶段、任务和修复行动的 Context Pack 使用包含完整六角色职责的协作策略，足以明确 lead，不需要每个阶段另行拼接角色 prompt；审查行动可附当前 reviewer 的接口视图。角色参考只提供视角，不改变 Core 行动、allowedFiles、verification、用户授权或结果 Schema。

四种拓扑分别表示当前 Agent 内联处理（`inline`）、由 conductor 派发 support 或 developer（`subagent`）、按依赖顺序逐段交付（`pipeline`）和并行收集独立只读贡献后由 lead 汇总（`mob`）。conductor 是唯一派发者，lead 整合贡献；reviewer 独立只读。普通 support 同时最多两个，build 允许 developer 子 Agent 实现当前纵向任务，但同一时间只能有一个业务写者，多任务按 Core 顺序串行。贡献优先使用 native 返回，临时材料保留到结果接受和恢复不再需要；模型选择遵循目标项目与宿主指引。

阶段 Skill 的 CLI 入口由 conductor 执行。已取得完整行动的子 Agent 直接消费 task/context/schema，不重新执行 `sdd status` 或阶段命令；缺少材料或需要恢复时由 conductor 重新获取后派发，避免再次派发任务或混用 PATH 中另一份 CLI。主 Agent 回传前核对全部实际 diff；build 的 evidence 仅记录当前 verification 的完整命令，其他调查或 Git 检查放在消息/贡献反馈中。TDD 的 RED 明确 `expectedFailure=true`、`passed=false`，GREEN 明确 `passed=true`，不得用自然语言“通过”代替字段。

除审查结果外，spec、plan、build task result 和 verify fix result 按当前 resultSchema 必填 `collaboration`，记录 `topology`、`lead`、`contributions` 和 `limitations`。`schemas/collaboration.schema.json` 是单一完整定义，并由 Core 嵌入宿主结果 Schema。每项贡献包含 `id`、`role`、`summary`、`inputs`、`feedback`，且 inputs 只引用已完成的前序贡献 id；pipeline 后段 inputs 引用前段贡献 id，mob 记录不同角色之间的交叉反馈，inline 可以没有贡献，subagent 至少保留一项贡献。

复杂风险优先独立审查，能力缺失必须显式自查。Core 下发 `AGENT_REVIEW_EXECUTION` 时，宿主使用原命令的 `--result-json` 回传 `READY` 或 `NOT_READY`，并声明 `independent`、`self` 或 `self_fallback`；结果可选回传与规格相同的七类 `riskFactors`。发现新风险时 Core 原子追加风险、生成新 targetHash，并要求同一 reviewer 重新独立审查；旧审查记录保留为历史，当前 candidate 按新风险重新判断；产品审查通过后才进入架构审查，不能复用旧 self/self_fallback 结论。只有没有可用派发时才填写 `fallbackReason`；审查不完整不能改称 self_fallback，第二次仍不完整以退出码 8 阻断并保留行动。规格候选先审查再落唯一 `spec.md`；verify 先完成确定性门禁再审查，QUALITY targetHash 排除 Core 生成的语义附录并绑定 spec/plan/tasks/fixes/workspace；归档前重新核对审查绑定的版本/哈希。质量自动修复仍只有一轮。

## 持续推进与恢复

用户要求完整实现时，五个阶段 Skill 在已有授权内顺序推进到交付；阶段切换不再机械确认。用户只要求分析、规格、计划或验证时遵守该范围。必要的新决策与事实仍需澄清，多任务目标未明和质量修复预算耗尽仍需用户决定。

`sdd-spec` 在项目未初始化时先执行对应宿主的初始化。继续等待中的规格时不再次传入需求文本，以免创建重复变更；等待计划或任务时再次执行原命令取得同一行动。CLI 本身不会启动 AI，也不会代替宿主运行业务测试。

## 需求澄清的轮次与结束条件

一轮优先一题，独立且相关的问题可以合并为最多三题；这是单轮限制，总轮数不设上限。每轮回答后更新已确认结论，并检查剩余歧义和回答引出的新问题。用户暂停就停止，恢复后接续未解决的问题，不重复已知答案。

2–3 个互斥选项只用于真实的方案取舍，应说明差异和影响。事实信息、实例或尚未形成候选的需求使用开放问题，允许自由补充，不强行凑选项。

目标、范围及排除项、可执行验收、重要边界与失败行为、关键技术决策充分明确，且不存在阻塞规格或实施的关键歧义时，才结束澄清并提交规格。回答满三题不是完成条件；“绝对清楚”也不是可验证标准，不能据此无限追问不影响结果的细节。用户已明确授权自主决定的实现细节无需重复审批。

可复核的交互案例：

- 尚有四项独立关键决定：完成首轮后继续下一轮，不提前生成规格。
- 用户已选方案但缺少真实业务样例：保留原决定，以开放问题补样例，不让用户重新选方案。
- 范围、验收和边界都明确：简要归纳后提交规格，不再为凑题数提问。

模板安装/刷新测试只证明 Codex、OMP 实际收到当前模板；上述澄清决策属于宿主行为，不能用字符串断言冒充多模型会话验证。

## 工作目录与临时材料

此约束适用于两端全部五个阶段。工作目录细则集中在 [workspace.md](../assets/policies/workspace.md)，协作细则集中在 [collaboration.md](../assets/policies/collaboration.md)；`init` 将各自内容写入每个 Skill 的 `references/workspace.md` 和 `references/collaboration.md`，各入口只保留前置提醒和阶段差异。参考缺失时先明确目标项目，再用对应宿主的 `init` 刷新；不能从另一个项目借用规则。

在首次 `status/init` 前先确定目标仓库绝对路径，后续 SDD 命令显式设置工具的 workdir/cwd；用户未明确目标且无法从已确认项目判断时先询问。权限不足按宿主授权机制处理，不能改到会话目录创建替代状态。

正式规格由目标项目 CLI 生成到 `.sdd/changes/<id>/spec.md`，默认不额外复制到会话目录，也不在仓库或会话目录累积 work/、outputs/、多版本 JSON/Markdown。用户明确要求导出或保留时遵循授权位置，导出副本不成为第二份权威规格。

中间 JSON、核对清单、命令输出优先留在内存；需要落盘时用安全临时目录机制创建系统临时目录下的独立任务目录。成功回传并核对正式文档后，按具体路径清理本任务创建且不再需要的临时材料；暂停或失败时仅保留恢复必需材料并说明位置、用途。不得自动清理用户既有文件、其他任务材料或宽泛目录。

这是宿主行为约束，不是 CLI 对任意文件写入的沙箱或自动清理器；不会扫描删除历史会话目录。

阶段边界：spec/plan 的正式文档由 CLI 接收结果后生成；build 的源代码、测试、迁移与计划内产物属于交付物；verify 保留回传、复核和故障定位需要的证据；archive 不授权删除业务文件、已接受证据或正式归档材料。公共“清理临时材料”不能覆盖这些边界。若流程明确提供隔离 worktree，业务编辑与验证在该工作区执行，SDD 控制命令仍在所属项目执行。

## 规格依据与语义审查

规格行动统一下发 [规格规则](../assets/policies/specification.md)，宿主在首次生成、恢复和修订时都应执行。澄清中维护简短的“结论—来源—状态”：

- 用户确认：原始需求、明确回答或授权；推荐项、沉默、只回答另一题不算确认。
- 代码事实：附文件、符号或契约出处。现有阈值、过滤、去重方式是旧实现，不自动成为新业务要求。
- 设计建议：标明 Agent 选择及理由。授权内的实现细节自主决定，会改变业务结果的取舍须澄清。
- 待确认：说明缺口与影响。阻塞问题未解决不得提交，不能藏到风险或默认值里；非阻塞外部依赖写清责任与验证条件。

回传前，宿主按原始需求和各轮回答做正向覆盖、反向查据、跨章节一致性及可实施性检查。修订须同时检查需求、验收、接口、数据模型、决策和测试策略，保留有效结论，清除已否定规则；之前被 Core 接收的规格仍可能包含误判，不能整体当作用户确认。

依据写入既有需求描述、代码现状、决策理由与接口字段，不另建第二份权威文档。外部契约区分已查证和拟议内容；接口细节深度按实际联调需要决定。Core 只做确定性结构校验，不验证用户是否真正确认，也不保证模型遵循提示。回传成功不能作为“语义审查通过”的证据。

共享规则编译进二进制，Skill 随 `sdd init` 下发。仅修改本仓库源码不会更新已安装 CLI 或其他项目的 Skill；采用新行为需安装包含该改动的构建，并在目标项目刷新对应宿主资产。

## 多任务交互契约

所有需要 change 的阶段 Skill 在执行前运行 `sdd status --json`：

1. 用户明确 changeId：使用该目标。
2. 只有一个活动 change：可以继续唯一任务。
3. 存在多个活动 change 且用户未明确：展示候选标题与阶段并询问；不得运行写命令，不得选择最近任务。
4. `status` 和 `codebase` 是项目级入口，不需要选择。

Core 同时执行相同规则，防止 Skill 提示遗漏时写错任务。

## 行动协议

- `AGENT_PHASE_EXECUTION`：调查真实代码，必要时询问用户，生成 resultSchema JSON；规格候选必须经过后续审查后才落盘；禁止修改业务文件。
- `AGENT_TASK_EXECUTION`：只修改 allowedFiles，执行任务内部 steps 和全部 verification，按行动中的完整 resultSchema 提交 TaskExecutionResult。
- `AGENT_FIX_EXECUTION`：只修复质量报告阻断项，行动带 `userAuthorized` 和完整 materialized `resultSchema`，提交 FixResult；自动轮次耗尽后普通 verify 保持阻断，只有获得用户明确授权后才能用 `sdd verify --continue` 开启下一轮。
- `AGENT_REVIEW_EXECUTION`：只读核对绑定的规格候选或质量报告，按完整 resultSchema 提交 READY/NOT_READY；原命令使用 `--result-json` 恢复，不能直接编辑正式文档或业务文件。

统一规格结果必须带 `reviewPolicy`：`riskFactors` 使用 `cross-module`、`external-contract`、`concurrency`、`transaction-consistency`、`permissions`、`sensitive-data`、`irreversible-data` 枚举，低风险可以为空但不能省略；`rationale` 记录判断依据。Core 根据该策略决定是否要求独立 reviewer。

宿主只向用户解释目标、决策、修改、验证、风险和选择问题。CLI JSON、Context Pack、Policy Bundle、runtime 路径、change/run/task 标识仅供内部处理，除非用户明确要求排障原始信息。

计划行动内嵌任务 Schema；command/args、testSeam 和文件范围的含义以该 Schema 和 CLI 文档为准。Core 核对提交证据的结构、一致性和 Git 事实；真实执行测试、审查业务语义仍是宿主职责，不能预填成功输出。

## 面向用户的沟通

五个阶段 Skill 都以业务结果、进度、验证和阻塞为输出内容，内部标识和结果 JSON 由 Agent 处理。多任务让用户按业务标题选择；需求澄清的选项应说明各方案影响，不要求用户选择内部状态或填写协议字段。

质量阻断先解释具体问题及影响，再询问是否授权额外一轮修复。用户可以手动修复，但普通 `sdd verify` 仍保持阻断；必须先获得用户明确授权并使用 `--continue` 重新进入修复验证链，不能把它当作默认授权或唯一出路。初始化时保存的目录结构选择会传入规格上下文，宿主应遵守已有选择。
