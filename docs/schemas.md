# Schema 说明

正式 JSON Schema 位于仓库根目录 `schemas/`，Rust Core 在编译时内嵌并执行校验。当前保留七个事实模型：

| Schema | 对应数据 |
| --- | --- |
| `state.schema.json` | `.sdd/runtime.json` 的 `state` 节点 |
| `runtime.schema.json` | `.sdd/runtime.json` 顶层统一运行数据 |
| `config.schema.json` | `.sdd/runtime.json` 的当前配置节点 |
| `task.schema.json` | runtime 计划中的任务定义 |
| `task-result.schema.json` | runtime 运行级任务结果 |
| `report.schema.json` | runtime 中的 verify/review 报告 |
| `artifact.schema.json` | runtime 中的制品条目 |

runtime 内的规格、设计、计划和归档模型沿用各自的 `schemaVersion` 字段。状态、runtime、config、任务、任务结果和报告会在关键读写边界执行结构校验（required/type/enum/const/pattern/minLength/minItems/minimum/uniqueItems/propertyNames/additionalProperties/$ref/oneOf/anyOf）；数组元素也按 `items` 递归校验。runtime 读取还会校验 state、config、artifacts、index、changes、runs、reports、loop 与归档之间的引用和阶段不变量，拒绝未知顶层字段、悬空 ID、重复诊断和错误聚合形状。任务定义在读取计划时执行 `task.schema.json` 校验。

索引节点固定为一条 CodeGraph 诊断、摘要和更新时间。诊断中的 `installed`/`version`、`indexed`/`degraded`/`reason` 必须彼此一致；降级原因还必须与工作流状态完全相同，provider、索引状态和摘要首行来源标记也必须对应。

每个业务 `run` 必须显式绑定现存的 `changeId`；当前 `currentRunId` 与 `currentChangeId` 必须属于同一变更。业务事件只接受当前 `REQUIREMENT_REVISED` 精确结构并同时绑定 run/change，未知字段或跨变更事件会在 runtime 边界拒绝。

计划任务只使用 `acceptanceCriteria`，旧 `acceptance` 字段已删除；`acceptanceCriteria` 固定列出需求场景 ID 与标题，`doneCriteria` 描述当前 RED/GREEN/REFACTOR/VERIFY 阶段的完成条件，两者不再重复。任务读取边界会复核 ID/阶段、重复节点、缺失依赖、依赖环、`expectedNewFiles`/`testSeam` 文件子集和验证命令。计划本身不再保存永远为 `PENDING` 的重复状态，唯一任务状态源是 `WorkflowState.tasks`，且只包含 `PENDING` / `BUILDING` / `DONE` / `FAILED`。任务结果的 evidence 只接受 `command-run`，不接受裁决层不会消费的旧证据类型或 `args` / `file` 字段；`taskId` 必须与当前 RED/GREEN/REFACTOR/VERIFY 任务格式一致，且 `verification` 必须不重不漏地覆盖全部计划命令。

`state.pendingAgentTask` 与 `state.workspace` 使用精确嵌套 schema，不接受未知或缺失字段。可用 Git 基线必须包含 40/64 位十六进制 OID、唯一文件列表和同键 SHA-256 表；不可用基线只能包含 `available=false`。workspace 的 branch/worktree 必须同时为空或同时为非空字符串，任务状态对象的键必须符合当前 `TASK-000-PHASE` 格式；Core 还会交叉校验文件与哈希键集合及阶段关系。

`config` 只接受 `schemaVersion`、`hostAdapter`、`workflow.gitIsolation`（以及可选的 `structurePolicy`）、`quality.ocr`、`contextPack.maxSizeKb` 和 `audit`。`maxSizeKb` 是代码库摘要的 UTF-8 字节上限。所有必填值必须存在且类型、枚举和正整数约束正确；未知字段、旧版本和旧配置位置会在读写边界直接返回 `E_STATE_CORRUPTED`，不迁移、不忽略、也不回退默认值。


## 报告字段

`report.schema.json` 中 `issues[]` 的 `code`、`severity`、`message` 为必填，其余均为可选：`file`、`origin`、`category`、`startLine`、`endLine`、`existingCode`、`suggestionCode`。OCR 后端产生的 finding 带 `origin="ocr"`，并可选携带定位（`startLine`/`endLine`）、分类（`category`）和建议代码（`suggestionCode`）；确定性 finding 保持原始格式。报告同时以 `minimality.ocr.status` 记录 OCR 结果：`completed`、`not-found`、`unavailable`、`failed`、`invalid-output`、`off`、`skipped` 或 `blocked-by-deterministic-review`。

OCR 适配器只接受当前 `ocr.run-manifest/v1` 输出：顶层 `status`、`llm`、`summary`、`tool_calls`、`comments`、`manifest` 必须完整且无未知字段；运行 ID、终态、操作类型、finding 数、审查文件数、工具调用统计、变更路径和行号必须交叉一致。旧版 `success`、`session_id` 或缺省统计字段不会被兼容解析。

状态写入使用同目录唯一临时文件、文件与目录同步、重命名和 `runtime.json.bak` 恢复，并维护主文件与备份各自的 SHA-256 校验和。缺失或不匹配的校验和、损坏、符号链接或版本不匹配都会返回 `E_STATE_CORRUPTED`，不会猜测内容继续执行。

## 校验

```bash
cargo test -p sdd-core --test schema_validator --test state_store --test artifact_store
```
