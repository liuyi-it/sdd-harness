---
name: sdd-new
description: 用户要为一个新的软件变更建立可验收规格时使用。
---

# SDD New

用用户原始需求运行 `sdd new "<需求>" --json`。收到 `AGENT_PHASE_EXECUTION` 后调查真实代码；只有无法从仓库确认且会改变目标、范围或验收的决策才询问用户。生成符合 resultSchema 的完整规格并用 `sdd new --change <id> --result-json '<JSON>' --json` 回传。此阶段不得修改业务文件。
