---
description: 用统一 Spec 流程处理自然语言需求
---

使用 sdd-spec skill 处理下面的自然语言需求。先运行 status；若是修订已有任务且存在多个活动任务而用户未指定目标，必须询问。按 `spec → plan → build → verify → archive` 推进，统一 spec 阶段同时澄清需求和技术设计，阶段行动由宿主生成结构化结果回传 Core。只有无法从代码确认且会改变目标、范围、验收或方案的决策才询问。不要直接修改 `.sdd/` 或展示内部 JSON。

$ARGUMENTS
