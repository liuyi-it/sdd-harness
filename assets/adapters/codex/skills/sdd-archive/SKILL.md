---
name: sdd-archive
description: 用户要归档一个已经通过统一质量门禁的 SDD 任务时使用。
---

# SDD Archive

先运行 `sdd status --json`；多任务且未指定时询问。只有目标 change 为 QUALITY_READY 时运行 `sdd archive --change <id> --json`。归档前不修改业务文件；完成后汇报归档结论和仍在进行的其他任务。

对用户简洁汇报业务结果、当前进度、验证和阻塞；内部标识、结果 JSON 和完整命令输出留给 Agent 处理。需要选择任务时展示需求标题与阶段，让用户按标题选择。
