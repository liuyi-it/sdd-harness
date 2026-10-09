---
description: 创建新的 SDD 需求并生成统一规格
---

使用 sdd-spec skill，先读取该 Skill 的 `references/workspace.md` 和 `references/collaboration.md`，将 `$ARGUMENTS` 作为原始需求创建新 change。先统一澄清需求与技术方案并按 resultSchema 回传含 `collaboration` 的候选，再接受 Core 的规格审查行动并用原命令 `--result-json` 回传审查结果；不得在规格审查完成前写入 `spec.md` 或修改业务文件。

$ARGUMENTS
