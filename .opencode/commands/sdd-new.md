---
description: 创建新的 SDD 需求并生成规格
---

先用一句话复述需求的目标和边界，再使用 sdd-harness skill 执行 `sdd new --json`。若返回 `CLARIFYING`，按 round 一次只问当前最重要的问题，优先目标、范围和验收；回答后使用 `sdd new --answers '<JSON>'` 继续，不要猜测未确认的业务选择或创建新的变更。

$ARGUMENTS
