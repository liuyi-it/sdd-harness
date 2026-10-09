---
description: 统一澄清需求与技术方案并生成唯一规格
---

使用 sdd-spec skill，先读取该 Skill 的 `references/workspace.md` 和 `references/collaboration.md`，处理 `$ARGUMENTS`：新需求进入 `sdd spec`，修订已有 change 时进入 `sdd change` 后的统一规格阶段。先完成必要澄清并回传同时包含需求规格、技术设计和 `collaboration` 的候选，收到 Core 的规格审查行动后再用原命令回传审查结果；不得在审查完成前修改业务文件或直接编辑 `.sdd/`。

$ARGUMENTS
