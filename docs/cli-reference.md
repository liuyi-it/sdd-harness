# CLI 命令参考

## 全局参数

| 参数 | 说明 |
| --- | --- |
| `--json` | 输出稳定 JSON，供宿主 Agent 使用 |
| `--cwd <path>` | 指定项目根目录 |
| `--change <id>` | 指定目标 change；多个活动任务时必填 |
| `--timeout <seconds>` | 锁等待或外部命令超时 |

全局参数可放在子命令前后。未知参数会被 CLI 或 Core 拒绝。

CLI 不自行启动模型。普通文本面向用户显示中文阶段和操作提示，`--json` 返回宿主完成工作所需的完整协议。调用成功且返回 Agent 行动表示正在等待宿主处理，不能据此宣布业务实现完成。

## `sdd init`

初始化 Runtime、宿主资产和代码库索引。终端默认 Codex；OMP 宿主内部调用 `--host-adapter omp`。用户无需交互选择宿主。

全新与重复初始化只在 `.sdd/` 中持久化 `runtime.json`、`lock`。状态内嵌校验并原子提交，不生成自动备份、独立 `.sha256` 或锁诊断文件；损坏时停止而不回退。旧状态格式不自动迁移或清理，需用户决定如何处理。

空项目可以直接继续描述需求，目录结构在规格阶段确定，无需先填写配置。可选 `--structurePolicy user-defined` 将“使用用户指定结构”的约束传入规格行动；`free-design` 则允许 Agent 按需求设计结构，并优先遵循已有代码约定。

```bash
sdd init
sdd init --structurePolicy free-design
```

## `sdd status`

只读查看活动任务。可用 `--change` 查看一个已归档或活动 change。多个活动任务且未指定时，状态为 `MULTIPLE_CHANGES`，data 中包含候选，不返回猜测的 next。

文本模式完整显示候选标题、标识和阶段；指定目标时显示任务完成进度，不因底层状态 JSON 过长而隐藏列表。等待中的 `next` 指向重新获取行动的命令，例如 `sdd plan --change <id>`，而非提交空的占位 JSON。

修订等待期间标题采用最新输入；显式查看已归档任务仍会显示业务标题。JSON 中 `selectedChange` 保存所选任务摘要；质量阶段还携带 `report`，文本模式显示具体问题和任务进度。

多任务冲突直接给出带标题、阶段和标识的候选；选错阶段时，恢复命令保留 `--change`，避免用户再次落入多任务选择。输入不存在的标识会提示运行 `sdd status` 查看候选。状态和错误码保持机器可读，错误原因使用中文描述。

```bash
sdd status --json
sdd status --change <id> --json
```

## `sdd spec <需求>`

创建新 change 并返回统一规格阶段行动。需求最长 32768 个 Unicode 字符；可用全局 `--change` 指定新 ID。宿主先复用已有回答，按主题持续多轮澄清关键歧义：每轮优先一题、最多三题，不限制总轮数；真实取舍提供可比选项，事实细节允许开放回答。目标、范围、可执行验收、重要边界和关键方案明确后才回传，不以题数作为结束条件，也不重复追问已知信息。Core 只生成一个同时包含可验收需求、场景和技术设计的 `spec.md`。

```bash
sdd spec "实现订单取消功能" --json
sdd spec --change order-cancel --result-json '<SpecPhaseResult JSON>' --json
```

每个新需求都创建独立 workflow，因此已有活动任务不会阻止新建。规格结果通过结构校验后先等待 `AGENT_REVIEW_EXECUTION`；审查 READY 后才写入唯一 `spec.md`，审查仍使用本命令的 `--result-json` 回传。

不带需求文本的 `sdd spec --change <id> --json` 恢复等待中的规格行动，不创建新变更。

## `sdd change <新需求>`

修订目标 change。多个活动任务时必须 `--change`；修订开始后重新生成同时包含技术设计的规格候选，候选审查 READY 后才更新唯一 `spec.md`，完成时作废所有派生制品。

计划等待中也可发起修订。等待修订结果期间再次传入非空新需求时更新输入；不传需求则恢复已有修订行动。行动携带修订前规格，供宿主保留仍有效的需求、场景和设计。

```bash
sdd change "订单取消需要记录操作者" --change order-cancel --json
sdd change --change order-cancel --result-json '<SpecPhaseResult JSON>' --json
```

## `sdd plan`

从含技术设计的统一规格生成纵向任务计划。Core 校验任务 ID、文件范围、依赖无环、验证命令安全，以及规格需求/场景的精确完整覆盖。

行动的 `resultSchema` 包含完整任务定义，无需宿主读取源码补全字段。计划结果版本为 4.0.0，并按 Schema 必填 `collaboration` 记录 `topology`、`lead`、`contributions` 和 `limitations`；`testSeam` 必须是允许范围内的具体文件路径；`forbiddenFiles` 无额外禁止项时允许 `[]`。

验证声明使用独立程序名与参数，例如 `{"command":"python3","args":["-m","unittest","-v"],"expected":"测试通过"}`。支持 Cargo 的 test/check/build/clippy/fmt、npm test 或 run test/lint/typecheck/build、Maven test/verify、Python unittest/pytest、pytest 和 node --test，均可携带参数。不接受 shell 拼接与发布命令。

Maven 选项可以位于质量生命周期前后，例如 `{"command":"mvn","args":["-B","-q","-s","settings.xml","clean","verify"],"expected":"测试与检查通过"}`。`-s`、`-f`、`-pl`、`-P`、`-D` 等选项按各自的参数消费规则解析，支持独立值、短选项附值和长选项 `=值`。`--color` 可省略值，有值时仅接受 `auto`、`always` 或 `never`。必须包含 `test` 或 `verify`；可附带 `clean`，拒绝 `install`、`deploy`、额外插件目标、未知选项、缺失参数，以及隐藏失败的 `--fail-never`。`-Bq` 等组合不会合并成两个布尔选项，仍须按 Maven 实际语义拒绝。规则依据 [Maven CLI 选项说明](https://maven.apache.org/ref/3.9.16/maven-embedder/cli.html)。门禁只检查声明的入口，宿主仍须确认实际执行了测试及其结果。

```bash
sdd plan --change <id> --json
sdd plan --change <id> --result-json '<PlanPhaseResult JSON>' --json
```

## `sdd build next|complete`

```bash
sdd build next --change <id> --json
sdd build complete \
  --change <id> \
  --task TASK-001 \
  --result-json '<TaskExecutionResult JSON>' \
  --json
```

`next` 返回 `AGENT_TASK_EXECUTION`。`complete` 要求 taskId 匹配 pending 任务、filesChanged 与 Git 事实一致、文件在计划范围内、全部 verification 不重不漏。TDD 任务完成时必须同时包含 expectedFailure=true 的失败证据和最终通过证据。

任务行动包含完整 `resultSchema`。TaskExecutionResult 还必须按 Schema 记录本次协作的 `topology`、`lead`、`contributions` 和 `limitations`；验证结果的程序名和参数数组必须逐项匹配计划，不能仅靠拼接后的命令字符串相同。宿主必须真正执行命令并保留输出。`tasks.md` 是计划定义，实时进度以 `sdd status` 为准。

## `sdd verify`

统一执行可追溯性、任务证据、Git 范围、敏感信息和依赖计划检查。

```bash
sdd verify --change <id> --json
sdd verify --change <id> --result-json '<FixResult JSON>' --json
sdd verify --change <id> --continue --json
```

确定性检查通过后可能返回 `AGENT_REVIEW_EXECUTION`，审查目标为当前质量报告，宿主按原命令 `--result-json` 回传 READY/NOT_READY。审查结果必须声明 `independent`、`self` 或 `self_fallback`，可选回传与 spec `reviewPolicy` 相同的七类 `riskFactors`，并核对 Core 下发的版本/哈希绑定。发现新风险时 Core 原子追加风险、生成新 targetHash，并要求同一 reviewer 重新独立审查；旧审查记录保留为历史，当前 candidate 按新风险重新判断，产品通过后才进入架构审查，不能复用旧 self/self_fallback。QUALITY targetHash 绑定确定性报告（排除 Core 生成的语义附录）以及 spec、plan、tasks、fixes、workspace。不完整审查只能由 independent 重派一次，第二次仍不完整以退出码 8 阻断并保留行动；完整结果不可覆盖，重复提交相同结果由原命令幂等恢复。首次质量失败返回带 `userAuthorized` 和完整 materialized `resultSchema` 的 `AGENT_FIX_EXECUTION`，并消耗一轮修复预算。修复后自动重新评估；仍失败进入 `QUALITY_BLOCKED`。普通 `sdd verify` 在 `QUALITY_BLOCKED` 保持阻断；即使用户手动修复，也必须先获得明确授权并用 `sdd verify --continue` 重新进入修复验证链，不能用普通 verify 直接开启新自动修复，且 `--continue` 不能与 `--result-json` 同时使用。

首次修复行动的 `data.report` 和被阻断后的状态都提供质量报告。Fix action 的 `userAuthorized` 表示是否有用户授权额外轮次；其完整 materialized `resultSchema` 随行动下发，宿主不需要猜测或拼装字段。FixResult 必须按 Schema 记录本次协作的 `topology`、`lead`、`contributions` 和 `limitations`。普通文本显示具体问题及关联文件；自动修复预算耗尽后明确要求用户选择手动修复并授权 `--continue`，或保持阻断。手动修复本身不会让普通 `sdd verify` 直接重入修复验证链，`--continue` 也不代表默认授权。

## `sdd archive`

仅允许 `QUALITY_READY`。归档前重新核对 Git 指纹、审查目标的版本/哈希绑定和任务完成状态，生成单一 `archive.md`。

```bash
sdd archive --change <id> --json
```

## `sdd codebase`

```bash
sdd codebase status --json
sdd codebase doctor --json
sdd codebase index --json
sdd codebase query "OrderService 取消订单调用链" --intent impact --json
sdd codebase rebuild --json
```

`codebase` 是项目级命令，不选择 change。`query` 必须带非空查询；`intent` 只对 query 有效。

文本模式直接显示提供者的可用状态、降级原因以及查询正文，不倾倒内部 JSON，也不因 JSON 超过 512 字符而隐藏结果。文件扫描本身的数量上限及其提示仍保留；`--json` 继续返回完整结构化结果。

## 已删除命令

当前版本删除 `sdd new`、`sdd design`、`sdd auto` 和 `sdd review`，也删除 `--answers`、`--non-interactive` 与计划依赖的 CLI 注入参数。旧命令不会作为兼容别名保留。
