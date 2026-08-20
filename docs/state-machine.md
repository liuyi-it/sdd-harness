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

这些值属于当前稳定枚举；当前实现会持久化 `INITIALIZING`、`NEW_STARTED`、`CLARIFYING`、`BUILDING`、`BUILD_WAITING_AGENT` 和 `PAUSED`（auto 步骤失败或 `--stop` 时写入）。`FAILED` 为契约预留值，当前实现不持久化；其余过程值不会被每个命令短暂写入，调用方不能依赖这一点。

信息不足时 `new` 进入 `CLARIFYING`；`NEW_STARTED` 表示 `new` 已记录当前 `changeId`/`runId` 但尚未完成规格生成，恢复建议为 `sdd auto --resume`。用户可用 `sdd new --answers '<JSON>'` 或 `sdd auto --resume --answers '<JSON>'` 继续，不得新建第二个变更或直接编辑 `.sdd/`；`build next` 返回 Agent 任务后进入 `BUILD_WAITING_AGENT`；auto 步骤失败或用户 `--stop` 时进入 `PAUSED`（保留 failed_command/failed_reason/suggested_command，`sdd auto --resume` 恢复）。

## 恢复信息

`.sdd/runtime.json` 的 `state` 节点提供以下恢复字段；命令只在对应失败或中断信息存在时写入：

- `previousPhase`：最近一次稳定阶段（阶段推进时由 Core 自动维护）。
- `inProgressPhase`：被中断或失败的执行阶段。
- `failedCommand`：需要恢复的失败命令。
- `interruptedCommand`：预留字段，当前实现不写入。
- `suggestedCommand`：`sdd status` 返回的下一步建议。
- `tasks` / `artifacts`：任务和关键制品状态。

命令重试必须通过相同的状态校验，不能直接编辑 runtime 文件绕过前置条件。`NEW_STARTED` 缺少当前 `changeId` 或 `runId` 时返回 `E_STATE_CORRUPTED`，建议先执行 `sdd status`，不会猜测恢复对象。


## Loop 状态

`activeLoop` 与 loop 事件统一记录在 `.sdd/runtime.json` 的 `loop` 节点；任务运行结果记录在 `runs` 节点。

`auto` 在以下边界停止：

- `CLARIFYING`：等待用户回答。
- `BUILD_WAITING_AGENT`：等待 Agent 执行任务。
- `PAUSED`：auto 步骤失败或用户停止，等待恢复或人工决策。
- `ARCHIVED`：流程完成。

`activeLoop.status` 为 `RUNNING`/`WAITING_AGENT` 期间，会切换变更或阶段规划的手动写命令（`init`、`new`、`change`、`design`、`plan`、`codebase index/rebuild`）返回 `E_CONCURRENT_RUN`；`build`、`verify`、`review`、`archive` 与只读命令不受影响。

## Git 工作区

启用隔离时，`workspace` 保存 `branchName`、`worktreePath` 和 `baselineCommit`。`build`、`verify`、`review`、`archive` 以 worktree 为业务目录，但状态和制品仍写入控制根目录的 `.sdd/`。

## 注意事项

- 空项目初始化仍进入 `INDEX_READY`，未指定 `--structurePolicy` 时返回 `W_EMPTY_PROJECT`；显式选择 `free-design` 或 `user-defined` 后写入配置并消除该警告。
- 归档 marker 已成功写入但状态更新中断时，再次执行 `archive` 会验证哈希并收敛到 `ARCHIVED`。
- 状态损坏或版本不受支持时返回 `E_STATE_CORRUPTED`，不会自动猜测恢复。
