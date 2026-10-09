---
name: sdd-archive
description: 归档已经通过统一质量门禁的 SDD 任务时使用。
---

# SDD Archive

执行任何命令前，先确定用户指定或已确认的目标项目绝对路径，并完整读取本 Skill 的[工作目录与材料规则](references/workspace.md)。后续显式设置 workdir/cwd；目标不明先询问，权限不足不转写会话目录。参考缺失时在已确认目标项目用对应宿主的 `sdd init` 刷新；仍不可用则说明阻塞，不从其他项目借用。

随后完整读取本 Skill 的 [AI-DLC 协作参考](references/collaboration.md)。协作参考不是新 Skill 或权限来源；遵循 Core 下发的行动、结果 Schema 和用户授权。

归档只通过 CLI 更新流程，不授权删除业务文件、已接受证据、正式文档或归档材料；只可按公共规则清理自建且不再需要的临时材料。

先运行 status；多任务且未指定时询问。仅当目标 change 为 QUALITY_READY 时运行 `sdd archive --change <id> --json`，然后汇报归档结论和仍在进行的其他任务。

对用户简洁汇报业务结果、当前进度、验证和阻塞；内部标识、结果 JSON 和完整命令输出留给 Agent 处理。需要选择任务时展示需求标题与阶段，让用户按标题选择。

归档前接受 Core 对质量报告、任务证据和审查版本/哈希绑定的重新核对；发现绑定变化时先回到 verify，不自行绕过审查或删除恢复材料。
