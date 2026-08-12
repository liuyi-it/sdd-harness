---
description: 创建新的 SDD 需求并生成规格
---

使用 sdd-harness skill 执行 `sdd new --json` 创建新需求；将用户提供的内容作为需求文本。若返回 `CLARIFYING`，按问题的 round 依次回答当前 frontier，并把回答写入 `sdd new --answers '<JSON>'`；问题应补齐目标、范围、验收、角色、接口、前置条件和失败路径，不要自行猜测。若返回 `NEW_STARTED` 或 `next` 为 `sdd auto --resume`，继续当前 `changeId`/`runId`，不要创建新的 `sdd new` 变更，也不要直接修改 `.sdd/`。不要展示 JSON、内部路径或状态码。

$ARGUMENTS
