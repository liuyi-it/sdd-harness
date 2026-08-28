---
name: sdd-build
description: 执行或继续 SDD 计划中的实现任务时使用。
---

# SDD Build

先运行 status；多任务且未指定时询问。用 `sdd build next --change <id> --json` 获取任务，只修改 allowedFiles，按 steps 实现并执行全部 verification；核对 diff 后用 `sdd build complete --change <id> --task <task-id> --result-json '<JSON>' --json` 回传，直到 BUILD_READY。
