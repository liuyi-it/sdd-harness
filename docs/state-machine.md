# 状态机说明

Core 是唯一允许推进状态的组件。稳定主路径为：

```text
NOT_INITIALIZED → INITIALIZING → INDEX_READY → NEW_STARTED → [CLARIFYING]
→ SPEC_READY → DESIGN_READY → PLAN_READY → BUILD_WAITING_AGENT
↔ BUILD_READY → VERIFY_READY → REVIEW_READY → ARCHIVED
```

## 过程状态

- 初始化：`INITIALIZING`。
- 规格：`NEW_STARTED`、`CLARIFYING`。
- Agent 边界：`BUILD_WAITING_AGENT`。
- 异常控制：`PAUSED`。

状态枚举只包含真实持久化路径，不保留未实现的过程状态或预留兼容值。命令内部正在执行的动作由 `inProgressPhase` 表示，不伪造额外工作流阶段。

信息不足时 `new` 进入 `CLARIFYING`；`NEW_STARTED` 表示 `new` 已记录当前 `changeId`/`runId` 但尚未完成规格生成，恢复建议为 `sdd auto --resume`。用户可用 `sdd new --answers '<JSON>'` 或 `sdd auto --resume --answers '<JSON>'` 继续，不得新建第二个变更或直接编辑 `.sdd/`；`build next` 返回 Agent 任务后进入 `BUILD_WAITING_AGENT`；auto 步骤失败或用户 `--stop` 时进入 `PAUSED`（保留 `failedCommand` / `failedReason` / `suggestedCommand`，`sdd auto --resume` 恢复）。
## 恢复信息

`.sdd/runtime.json` 的 `state` 节点提供以下恢复字段；命令只在对应失败或中断信息存在时写入：

- `previousPhase`：最近一次稳定阶段（阶段推进时由 Core 自动维护）。
- `inProgressPhase`：被中断或失败的执行阶段。
- `failedCommand`：需要恢复的失败命令。
- `failedReason`：与 `failedCommand` 成对存在的非空失败原因。
- `suggestedCommand`：`sdd status` 返回的下一步建议。
- `tasks`：与当前机器计划一一对应的任务状态；制品事实只保存在 runtime 的 `artifacts` 注册表中。

任务状态只允许 `PENDING` / `BUILDING` / `DONE` / `FAILED`。`BUILDING` 必须且只能对应 `pendingAgentTask.taskId`；pending 中的 Git 基线文件、哈希和 HEAD 必须结构完整且互相一致，离开 Agent 等待阶段后不得残留 BUILDING 任务。

命令重试必须通过相同的状态校验，不能直接编辑 runtime 文件绕过前置条件。`initialized`、阶段、索引状态、活动 ID、任务状态和失败字段必须彼此一致；需要活动变更的阶段若缺少对应 `changeId`、`runId` 或聚合数据，runtime 在读取边界直接返回 `E_STATE_CORRUPTED`。

## Loop 状态

`activeLoop` 摘要记录在 `state`，auto 运行记录与事件一一对应地保存在 `loop.runs` / `loop.events`；普通需求修订事件与任务结果归属对应的 `runs` 聚合。

归档完成后的幂等 `auto` 仍复用最后一次成功 run；一旦携带新需求开启新变更，必须创建新的 loop/run，不覆盖上一变更的成功运行历史。

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
