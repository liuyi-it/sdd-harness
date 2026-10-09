---
description: 执行 SDD 纵向实施任务
---

使用 sdd-build skill，先读取该 Skill 的 `references/workspace.md` 和 `references/collaboration.md`，执行目标 change 的下一个纵向任务，并在 task result 中按 Schema 记录 `collaboration`。允许 developer 子 Agent 实现当前任务，但同一时间只能有一个业务写者，多任务按 `build next` 顺序串行。多个活动任务且 `$ARGUMENTS` 未明确目标时必须先询问；严格遵守 allowedFiles 和 verification。

$ARGUMENTS
