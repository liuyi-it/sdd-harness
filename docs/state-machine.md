# 状态机说明

Core 是唯一允许推进状态的组件。稳定主路径为：

```text
NOT_INITIALIZED → INDEX_READY → SPEC_READY → DESIGN_READY → PLAN_READY
→ BUILD_READY → VERIFY_READY → REVIEW_READY → ARCHIVED
```


## 过程状态

- 初始化与索引：`INITIALIZING`、`INDEXING`。
- 规格与设计：`NEW_STARTED`、`CLARIFYING`、`DESIGNING`。
- 计划与构建：`PLANNING`、`BUILDING`、`BUILD_WAITING_AGENT`。
- 质量与归档：`VERIFYING`、`REVIEWING`、`ARCHIVING`。
- 异常控制：`FAILED`、`PAUSED`。

这些值属于稳定枚举；当前实现会持久化 `INITIALIZING`、`NEW_STARTED`、`CLARIFYING`、`BUILDING`、`BUILD_WAITING_AGENT`、`FAILED` 和 `PAUSED`。其余过程值为兼容保留值，调用方不能假设每个命令都会短暂写入对应过程值。

信息不足时 `new` 进入 `CLARIFYING`；`NEW_STARTED` 表示 `new` 已记录当前 `changeId`/`runId` 但尚未完成规格生成，恢复建议为 `sdd auto --resume`。用户可用 `sdd new --answers '<JSON>'` 或 `sdd auto --resume --answers '<JSON>'` 继续，不得新建第二个变更或直接编辑 `.sdd/`；`build next` 返回 Agent 任务后进入 `BUILD_WAITING_AGENT`；用户中断或需要扩大修复范围时进入 `PAUSED`。

## 恢复信息

`.sdd/runtime.json` 的 `state` 节点提供以下恢复字段；命令只在对应失败或中断信息存在时写入：

- `previousPhase`：最近一次稳定阶段。
- `inProgressPhase`：被中断或失败的执行阶段。
- `failedCommand` / `interruptedCommand`：需要恢复的命令。
- `suggestedCommand`：`sdd status` 返回的下一步建议。
- `tasks` / `artifacts`：任务和关键制品状态。

命令重试必须通过相同的状态校验，不能直接编辑 runtime 文件绕过前置条件。`NEW_STARTED` 缺少当前 `changeId` 或 `runId` 时返回 `E_STATE_CORRUPTED`，建议先执行 `sdd status`，不会猜测恢复对象。


## Loop 状态

`activeLoop` 与 loop 事件统一记录在 `.sdd/runtime.json` 的 `loop` 节点；任务运行结果记录在 `runs` 节点。

`auto` 在以下边界停止：

- `CLARIFYING`：等待用户回答。
- `BUILD_WAITING_AGENT`：等待 Agent 执行任务。
- `FAILED` / `PAUSED`：等待恢复或人工决策。
- `ARCHIVED`：流程完成。

## Git 工作区

启用隔离时，`workspace` 保存 `branchName`、`worktreePath` 和 `baselineCommit`。`build`、`verify`、`review`、`archive` 以 worktree 为业务目录，但状态和制品仍写入控制根目录的 `.sdd/`。

## 注意事项

- 空项目初始化仍进入 `INDEX_READY`，未指定 `--structurePolicy` 时返回 `W_EMPTY_PROJECT`；显式选择 `free-design` 或 `user-defined` 后写入配置并消除该警告。
- 归档 marker 已成功写入但状态更新中断时，再次执行 `archive` 会验证哈希并收敛到 `ARCHIVED`。
- 状态损坏或版本不受支持时返回 `E_STATE_CORRUPTED`，不会自动猜测恢复。
