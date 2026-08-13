---
description: 用 sdd-harness 处理自然语言需求
---

使用 sdd-harness skill 处理下面的自然语言需求；先复述目标，只有需要用户决策时提问。主 Agent 负责最终检查和审查。需要显式控制工作流时，使用 `/sdd.init`、`/sdd.status`、`/sdd.new`、`/sdd.change`、`/sdd.plan`、`/sdd.verify`、`/sdd.review` 或 `/sdd.archive`。若 Core 返回 `CLARIFYING`，按 round 一次只问当前最重要的问题；若返回 `next: sdd auto --resume`，继续当前 loop，不调用新的 `sdd new`，需要答案时使用 `sdd auto --resume --answers '<JSON>'`。设计、计划或执行涉及公开接口、数据迁移、权限安全、外部服务、删除覆盖或新增依赖时，先向用户摘要影响并确认。需求变化时使用 `/sdd.change`，不要直接编辑 `.sdd/`。禁止直接修改 `.sdd/` 或展示内部 JSON、阶段码和路径。

$ARGUMENTS
