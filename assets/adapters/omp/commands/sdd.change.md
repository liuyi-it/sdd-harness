---
description: 修订已有 SDD 需求并同步变更文档
---

使用 sdd-spec skill，先读取该 Skill 的 `references/workspace.md` 和 `references/collaboration.md`，修订目标 change：先运行 `sdd change`，再处理统一规格阶段。多个活动任务且 `$ARGUMENTS` 未明确目标时必须先询问；重新生成同时包含技术设计和 `collaboration` 记录的候选规格并先回传，收到 Core 的规格审查行动后再用原命令回传审查结果，READY 后才写入唯一 `spec.md`，不直接编辑 `.sdd/`。

$ARGUMENTS
