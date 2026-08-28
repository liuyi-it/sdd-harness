---
name: sdd-new
description: 为新的软件变更创建可验收规格时使用。
---

# SDD New

运行 `sdd new "<需求>" --json`。收到阶段行动后调查真实代码，只询问无法从仓库确认且会改变目标、范围或验收的决策；生成符合 resultSchema 的完整规格并用 `sdd new --change <id> --result-json '<JSON>' --json` 回传。不得修改业务文件。
