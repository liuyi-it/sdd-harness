---
name: sdd-change
description: 用户要修订一个进行中或已归档 SDD 任务的需求时使用。
---

# SDD Change

先运行 `sdd status --json`。用户未指定 change 且存在多个活动任务时必须询问。用 `sdd change "<新需求>" --change <id> --json` 开始修订，重新调查代码并生成完整规格，再以 `--result-json` 回传。旧设计、计划和质量制品由 Core 作废；不要直接编辑 `.sdd/`。
