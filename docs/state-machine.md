# 状态机说明

稳定主路径如下：

```text
NOT_INITIALIZED → INDEX_READY → SPEC_READY → DESIGN_READY → PLAN_READY
→ BUILD_READY → VERIFY_READY → REVIEW_READY → ARCHIVED
```

长耗时阶段会进入 `INITIALIZING`、`INDEXING`、`NEW_STARTED`、`DESIGNING`、`PLANNING`、`BUILDING`、`VERIFYING`、`REVIEWING` 和 `ARCHIVING`。存在未回答阻塞问题时使用 `CLARIFYING`，用户中断时使用 `PAUSED`，执行失败时使用 `FAILED`。

恢复逻辑依赖 `failedCommand` 或 `interruptedCommand`。其中 `previousPhase` 表示最近一次稳定阶段，`inProgressPhase` 表示被打断的执行阶段。`sdd status` 会回报当前保存的 `suggestedCommand`。
