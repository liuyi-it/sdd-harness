---
name: sdd-build
description: 用户要执行或继续 SDD 计划中的实现任务时使用。
---

# SDD Build

先运行 `sdd status --json`；多任务且未指定时询问。用 `sdd build next --change <id> --json` 获取一个任务，只修改 allowedFiles，按 steps 完成实现并执行全部 verification。核对真实 diff 后，用 `sdd build complete --change <id> --task <task-id> --result-json '<JSON>' --json` 回传。重复直到进入 BUILD_READY；不要自行扩大范围。
