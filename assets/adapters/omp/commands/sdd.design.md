---
description: 将技术方案并入统一规格
---

使用 sdd-spec skill，先读取该 Skill 的 `references/workspace.md` 和 `references/collaboration.md`，处理目标 change 的技术方案补充：通过 `sdd change` 回到统一规格阶段，先回传含 `collaboration` 的规格候选，收到规格审查行动后再用原命令回传审查结果，READY 后才将技术设计写入唯一 `spec.md`。多个活动任务且 `$ARGUMENTS` 未明确目标时必须先询问；此阶段不得修改业务文件。

$ARGUMENTS
