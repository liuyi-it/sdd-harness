# JSON Schema

所有 Schema 位于 `schemas/` 并编译进二进制。Core 的轻量校验器支持当前文件实际使用的 type、required、properties、additionalProperties、items、enum、const、pattern、minimum、minLength、minItems、uniqueItems、oneOf、anyOf 和同文档 `$ref`。

| Schema | 用途 |
| --- | --- |
| `runtime.schema.json` | Runtime 存储根结构，当前 schemaVersion 9，包含内嵌 checksum |
| `state.schema.json` | 项目初始化与索引状态，当前 schemaVersion 4 |
| `config.schema.json` | 宿主、Git 隔离、Context Pack 和审计限制，当前 schemaVersion 4 |
| `artifact.schema.json` | 制品类型、路径、输入和内容哈希 |
| `spec-result.schema.json` | Agent 回传的统一规格与技术设计结果，版本 6.0.0 |
| `spec.schema.json` | Core 持久化的 READY 统一规格模型，版本 6.0.0 |
| `plan-result.schema.json` | Agent 回传的计划与任务集合，版本 4.0.0 |
| `collaboration.schema.json` | spec、plan、task result 和 fix result 共用的协作记录 Schema，由 Core 嵌入各宿主结果 Schema |
| `task.schema.json` | 单个纵向任务 |
| `task-result.schema.json` | build 的任务执行结果 |
| `fix-result.schema.json` | verify 的受控修复结果 |
| `report.schema.json` | kind=quality 的统一质量报告 |

## 阶段结果

统一规格结果必须包含目标、included/excluded 范围、约束、Requirement/Scenario 模型和 `technicalDesign`。`technicalDesign` 必须包含真实代码现状、决策与理由、影响文件、接口或数据流、错误处理、测试、风险和回滚。计划结果必须包含全局约束、依赖决策和至少一个 task。spec、plan、build 的 task result 和 verify 的 fix result 还必须包含 `collaboration`，记录本次协作的拓扑、lead、贡献和限制。

Core 对 result JSON 先做 Schema 校验，再反序列化为强类型，并递归拒绝占位内容。计划还会逐个用 task schema 复验，检查文件范围矛盾、验证命令、依赖环和规格覆盖。

规格依据复用现有字段表达：`requirements[].statement` 记录业务规则与关键确认依据，`technicalDesign.currentState` 记录代码事实及出处，`decisions[].rationale` 区分用户确认和 Agent 设计理由，`interfaces` 区分已有契约和拟议契约。没有新增“语义已通过”的自报字段；这些依据的真实性由宿主核对会话和源码，Schema 不会判定。`SPEC_READY` 表示当前候选通过结构校验并且当前审查返回 READY 后落盘，仍不等于需求已逐项确认或语义正确。

规格结果还必须包含 `reviewPolicy`：`riskFactors` 为风险枚举数组，允许 `cross-module`、`external-contract`、`concurrency`、`transaction-consistency`、`permissions`、`sensitive-data`、`irreversible-data`；`rationale` 说明风险判断依据。低风险规格使用空数组，但不能省略字段。规格候选通过结构校验后先进入审查，`SPEC_READY` 只在审查 READY 后产生。

`plan-result.schema.json` 的 tasks 引用同目录 `task.schema.json`；内嵌校验和行动下发时共用展开后的 Schema。宿主拿到的 resultSchema 无需访问外部文件即可获知全部任务字段。构建行动同样附带完整 task-result Schema。

`collaboration` 的 `topology` 使用 `inline`、`subagent`、`pipeline` 或 `mob`；`lead` 和每条贡献的 `role` 使用共享角色名；`contributions` 的每项包含 `id`、`role`、`summary`、`inputs` 和 `feedback`，其中 `inputs` 只能引用本次记录中已经完成的前序贡献 id；`limitations` 记录能力、派发、外部依赖和验证边界。拓扑记录必须满足：inline 可无贡献，subagent 至少一项贡献，pipeline 至少两项且后项 inputs 引用前项 id，mob 至少两项不同角色并记录交叉 feedback。审查行动的 review result 仍以其独立 resultSchema 为准，不套用这组阶段协作记录。

`collaboration.schema.json` 是协作记录的单一完整定义；宿主收到的各阶段 resultSchema 已内嵌同一约束，不应另造宿主专用版本。

## Runtime 存储校验

存储 JSON 的根字段 `checksum` 必填，为 64 位小写十六进制 SHA-256。计算时移除该字段，再将剩余 JSON 值按对象键排序进行紧凑序列化，对其 UTF-8 字节计算摘要；数组顺序和字符串内容仍参与校验。纯缩进和对象键顺序不影响结果。该字段属于存储层，不加入 Core 的领域 `RuntimeDocument`。

状态与 checksum 一次原子提交，不生成 `.sha256`、`.bak` 或锁诊断文件；损坏时返回错误，不接受备份回退。旧格式在校验内容前按版本拒绝，不自动迁移或删除。宿主依然只能通过 CLI 更新状态，不应自行构造存储校验。

## 纵向任务

taskId 格式为 `TASK-001`。`executionMode` 为 `TDD` 或 `VERIFY_ONLY`。TDD 的 steps 至少包含 TEST、IMPLEMENT、VERIFY；VERIFY_ONLY 至少包含 VERIFY。`interfaces`、用户可见结果、验收标准和 testSeam 用于防止计划退化成模糊动作列表。

`testSeam` 为允许范围内的具体测试入口文件路径，不是自然语言说明。`forbiddenFiles` 允许空数组。验证命令的 `command` 仅填程序名，`args` 保存独立参数；结果回传要求程序和参数数组逐项匹配计划。

## 结果传输

阶段、任务、修复和审查行动都使用 `resultTransport=inline-json`，不接受 Agent 提供的任意结果文件路径。任务结果包含 evidence、verification 和 filesChanged；修复结果包含 fixId、status、verification 和 filesChanged。`AGENT_FIX_EXECUTION` 还携带 `userAuthorized` 和完整 materialized `resultSchema`，用于区分首次受控修复与用户授权的额外轮次。`AGENT_REVIEW_EXECUTION` 携带 reviewId、targetHash、target（`SPECIFICATION` 或 `QUALITY`）、reviewer、independentRequired、contextPack 和完整 resultSchema；审查结果为 READY/NOT_READY，声明 `independent`、`self` 或 `self_fallback`，并返回 findings 及对 Core 既有发现的 rechecks，可选 `riskFactors` 使用与 `reviewPolicy` 相同的七类枚举。只有缺少可用派发时才允许 fallbackReason；不完整结果不能把 independent 改称 self_fallback，第二次不完整以退出码 8 阻断并保留行动。

`SPECIFICATION` 的 targetHash 绑定当前候选及输入；`QUALITY` 的 targetHash 绑定确定性质量报告（排除 Core 生成的语义审查附录）以及 spec、plan、tasks、fixes、workspace。新风险会使 Core 原子追加风险并生成新目标哈希，使用同一 reviewer 重新独立审查；旧审查记录和 findings 保留为历史，当前 candidate 按新风险重新判断，不能覆盖旧记录或复用旧结论。历史审查结果和 findings 单调追加，完整结果不可覆盖，重复提交相同完整结果由原命令幂等恢复。

## 报告

统一报告 `kind` 固定为 `quality`。issues 的必填字段为 code、severity、message；可选 file、category、startLine、endLine、existingCode、suggestionCode、origin。minimality 保存 Git 指纹和实际变更文件等可复核事实。

CLI 行动和状态查询会复用该报告：首次质量修复行动通过 `data.report` 提供；选中质量阶段的任务时，状态也通过 `data.report` 提供。`data.selectedChange` 为选中任务的标题、阶段、标识与恢复命令摘要；不存在适用质量报告时 report 为 null。它们是输出视图，不新增 Runtime Schema 或第二份状态。
