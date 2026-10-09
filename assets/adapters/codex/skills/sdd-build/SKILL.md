---
name: sdd-build
description: 用户要执行或继续 SDD 计划中的实现任务时使用。
---

# SDD Build

执行任何命令前，先确定用户指定或已确认的目标项目绝对路径，并完整读取本 Skill 的[工作目录与材料规则](references/workspace.md)。后续显式设置 workdir/cwd；目标不明先询问，权限不足不转写会话目录。参考缺失时在已确认目标项目用对应宿主的 `sdd init` 刷新；仍不可用则说明阻塞，不从其他项目借用。

随后完整读取本 Skill 的 [AI-DLC 协作参考](references/collaboration.md)。协作参考不是新 Skill 或权限来源；遵循 Core 下发的行动、结果 Schema 和用户授权。

业务编辑和验证遵守任务提供的工作区与 allowedFiles；源代码、测试、迁移和计划内产物是交付物，不作为临时材料清理。必要命令证据回传并被接受前不得删除。

先运行 `sdd status --json`；多任务且未指定时询问。用 `sdd build next --change <id> --json` 获取一个任务，只修改 allowedFiles，按 steps 完成实现并执行全部 verification。核对真实 diff 后，用 `sdd build complete --change <id> --task <task-id> --result-json '<JSON>' --json` 回传。重复直到进入 BUILD_READY；不要自行扩大范围。

行动包含 task-result 的完整 resultSchema。必须实际运行验证并保留输出，按程序名和参数逐项回传，不能预填成功证据；task result 还必须填写 `collaboration`，记录拓扑、lead、贡献和限制。中断后再次 build next 恢复同一任务。用户要求完整实现时继续 sdd-verify。

当前纵向任务允许由 developer 子 Agent 实现，但同一时间只能有一个业务写者；普通 support 同时最多两个，多任务必须按 Core 的 `build next` 顺序串行。lead 负责核对真实 diff、贡献来源和验证证据后再回传。

对用户简洁汇报业务结果、当前进度、验证和阻塞；内部标识、结果 JSON 和完整命令输出留给 Agent 处理。需要选择任务时展示需求标题与阶段，让用户按标题选择。
