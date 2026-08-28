---
name: sdd-archive
description: 归档已经通过统一质量门禁的 SDD 任务时使用。
---

# SDD Archive

先运行 status；多任务且未指定时询问。仅当目标 change 为 QUALITY_READY 时运行 `sdd archive --change <id> --json`，然后汇报归档结论和仍在进行的其他任务。
