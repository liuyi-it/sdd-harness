---
description: 用 sdd-harness 处理自然语言需求
---

使用 sdd-harness skill 处理下面的自然语言需求。先运行 status；若有多个活动任务且用户未指定目标，必须询问。按 `new → design → plan → build → verify → archive` 逐步推进，阶段行动由宿主生成结构化结果回传 Core。只有无法从代码确认且会改变目标、范围或验收的决策才询问。不要调用已删除的 auto/review，也不要直接修改 `.sdd/` 或展示内部 JSON。

$ARGUMENTS
