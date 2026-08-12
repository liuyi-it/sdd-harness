---
description: 用 sdd-harness 处理自然语言需求
---

使用 sdd-harness skill 处理下面的自然语言需求；主 Agent 负责最终检查和审查。需要显式控制工作流时，使用 `/sdd.init`、`/sdd.status`、`/sdd.new`、`/sdd.change`、`/sdd.plan`、`/sdd.verify`、`/sdd.review` 或 `/sdd.archive`。若 Core 返回 `CLARIFYING`，按 round 回答当前 frontier 中的问题；若返回 `next: sdd auto --resume`，继续当前 loop，不调用新的 `sdd new`，需要答案时使用 `sdd auto --resume --answers '<JSON>'`。需求变化时使用 `/sdd.change`，不要直接编辑 `.sdd/`。禁止直接修改 `.sdd/` 或展示内部 JSON、阶段码和路径。

$ARGUMENTS
