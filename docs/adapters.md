# Agent 接入

## Codex

`sdd init` 默认写入 `.agents/skills/`：

- `sdd-spec`：复用已有决策，以选择题和开放问题持续多轮澄清关键歧义，再生成规格与技术设计；
- `sdd-plan`、`sdd-build`、`sdd-verify`、`sdd-archive`：计划、实施、质量门禁和归档的阶段入口。

Codex 新初始化只写入这五个 Skill。`init`、`status`、`change` 和 `codebase` 仍是 CLI 命令，但不再各自占用 Skill。本项目不再安装专用 subagent 配置；是否使用通用 subagent 由宿主和用户决定，小任务不会被强制拆给多个角色。

## OMP

OMP 宿主运行 `sdd init --host-adapter omp`，写入：

- `.omp/skills/` 下与 Codex 同名的五个 Skill；
- `.omp/commands/sdd.md` 自然语言入口；
- `/sdd.init`、`/sdd.status`、`/sdd.spec`、`/sdd.new`、`/sdd.change`、`/sdd.design`、`/sdd.plan`、`/sdd.build`、`/sdd.verify`、`/sdd.archive`、`/sdd.codebase` 全部显式命令。

OMP slash command 是快捷入口，不计入 Skill 数量。`/sdd`、`/sdd.new` 和 `/sdd.design` 会进入 `sdd-spec`；`/sdd.change` 先运行 `sdd change` 再进入同一统一规格阶段。新版本初始化不会扫描、删除或迁移项目中已存在的旧 Skill，旧资产由用户自行处理。

终端默认 Codex，OMP 自己传入隐藏宿主标识；用户无需在 `sdd init` 时选择。

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

## 多任务交互契约

所有需要 change 的阶段 Skill 在执行前运行 `sdd status --json`：

1. 用户明确 changeId：使用该目标。
2. 只有一个活动 change：可以继续唯一任务。
3. 存在多个活动 change 且用户未明确：展示候选标题与阶段并询问；不得运行写命令，不得选择最近任务。
4. `status` 和 `codebase` 是项目级入口，不需要选择。

Core 同时执行相同规则，防止 Skill 提示遗漏时写错任务。

## 行动协议

- `AGENT_PHASE_EXECUTION`：调查真实代码，必要时询问用户，生成 resultSchema JSON；禁止修改业务文件。
- `AGENT_TASK_EXECUTION`：只修改 allowedFiles，执行任务内部 steps 和全部 verification，按行动中的完整 resultSchema 提交 TaskExecutionResult。
- `AGENT_FIX_EXECUTION`：只修复质量报告阻断项，提交 FixResult；自动轮次耗尽后必须获得用户授权。

宿主只向用户解释目标、决策、修改、验证、风险和选择问题。CLI JSON、Context Pack、Policy Bundle、runtime 路径、change/run/task 标识仅供内部处理，除非用户明确要求排障原始信息。

计划行动内嵌任务 Schema；command/args、testSeam 和文件范围的含义以该 Schema 和 CLI 文档为准。Core 核对提交证据的结构、一致性和 Git 事实；真实执行测试、审查业务语义仍是宿主职责，不能预填成功输出。

## 面向用户的沟通

五个阶段 Skill 都以业务结果、进度、验证和阻塞为输出内容，内部标识和结果 JSON 由 Agent 处理。多任务让用户按业务标题选择；需求澄清的选项应说明各方案影响，不要求用户选择内部状态或填写协议字段。

质量阻断先解释具体问题及影响，再询问是否授权额外一轮修复。用户也可手动修复后重新验证，不能把 `--continue` 当作唯一出路或默认授权。初始化时保存的目录结构选择会传入规格上下文，宿主应遵守已有选择。
