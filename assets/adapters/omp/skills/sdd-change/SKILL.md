---
name: sdd-change
description: 修订一个进行中或已归档 SDD 任务的需求时使用。
---

# SDD Change

先运行 status；多个活动任务且用户未指定时询问。用 `sdd change "<新需求>" --change <id> --json` 开始修订，重新调查并生成完整规格，再以 `--result-json` 回传。不要直接编辑 `.sdd/`。
