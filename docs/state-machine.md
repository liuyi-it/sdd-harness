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

信息不足时 `new` 进入 `CLARIFYING`；`build next` 返回 Agent 任务后进入 `BUILD_WAITING_AGENT`；用户中断或需要扩大修复范围时进入 `PAUSED`。

## 恢复信息

`.sdd/state.json` 同时记录：

- `previousPhase`：最近一次稳定阶段。
- `inProgressPhase`：被中断或失败的执行阶段。
- `failedCommand` / `interruptedCommand`：需要恢复的命令。
- `suggestedCommand`：`sdd status` 返回的下一步建议。
- `tasks` / `artifacts`：任务和关键制品状态。

命令重试必须通过相同的状态校验，不能直接编辑状态文件绕过前置条件。

## Loop 状态

`activeLoop` 记录当前 auto 的 loopId、runId、状态和恢复标记；`.sdd/loop/runs/` 与事件文件记录每个步骤、决策和时间戳。

`auto` 在以下边界停止：

- `CLARIFYING`：等待用户回答。
- `BUILD_WAITING_AGENT`：等待 Agent 执行任务。
- `FAILED` / `PAUSED`：等待恢复或人工决策。
- `ARCHIVED`：流程完成。

## Git 工作区

启用隔离时，`workspace` 保存 `branchName`、`worktreePath` 和 `baselineCommit`。`build`、`verify`、`review`、`archive` 以 worktree 为业务目录，但状态和制品仍写入控制根目录的 `.sdd/`。

## 注意事项

- 空项目初始化可能进入 `CLARIFYING`，等待确认目录结构规范。
- 归档 marker 已成功写入但状态更新中断时，再次执行 `archive` 会验证哈希并收敛到 `ARCHIVED`。
- 状态损坏或版本不受支持时返回 `E_STATE_CORRUPTED`，不会自动猜测恢复。
