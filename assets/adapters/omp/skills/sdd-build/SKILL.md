---
name: sdd-build
description: 执行或继续 SDD 计划中的实现任务时使用。
---

# SDD Build

先运行 status；多任务且未指定时询问。用 `sdd build next --change <id> --json` 获取任务，只修改 allowedFiles，按 steps 实现并执行全部 verification；核对 diff 后用 `sdd build complete --change <id> --task <task-id> --result-json '<JSON>' --json` 回传，直到 BUILD_READY。

行动包含 task-result 的完整 resultSchema。必须实际运行验证并保留输出，按程序名和参数逐项回传，不能预填成功证据。中断后再次 build next 恢复同一任务。用户要求完整实现时继续 sdd-verify。

对用户简洁汇报业务结果、当前进度、验证和阻塞；内部标识、结果 JSON 和完整命令输出留给 Agent 处理。需要选择任务时展示需求标题与阶段，让用户按标题选择。
