---
name: sdd-archive
description: 用户要归档一个已经通过统一质量门禁的 SDD 任务时使用。
---

# SDD Archive

先运行 `sdd status --json`；多任务且未指定时询问。只有目标 change 为 QUALITY_READY 时运行 `sdd archive --change <id> --json`。归档前不修改业务文件；完成后汇报归档结论和仍在进行的其他任务。
