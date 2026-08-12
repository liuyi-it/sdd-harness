---
description: 创建新的 SDD 需求并生成规格
---

使用 sdd-harness skill 执行 `sdd new --json` 创建新需求；将用户提供的内容作为需求文本。若返回 `CLARIFYING`，只向用户提出必要问题，收到答案后执行 `sdd new --answers '<JSON>'`；若返回 `NEW_STARTED` 或 `next` 为 `sdd auto --resume`，继续当前 `changeId`/`runId`，不要创建新的 `sdd new` 变更，也不要直接修改 `.sdd/`。不要展示 JSON、内部路径或状态码。

$ARGUMENTS
