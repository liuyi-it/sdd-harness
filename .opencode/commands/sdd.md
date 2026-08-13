---
description: 用 sdd-harness 处理自然语言需求
---

使用 sdd-harness skill 处理下面的需求：先复述目标，只有需要用户决策时提问。项目未初始化时先执行 `sdd init --host-adapter opencode --json`；已初始化时使用 `sdd auto "<需求>" --json`。若返回 `CLARIFYING`，按 round 一次只问当前最重要的问题；若返回 `next: sdd auto --resume`，继续当前 loop，不创建新的变更。设计、计划或执行涉及公开接口、数据迁移、权限安全、外部服务、删除覆盖或新增依赖时，先向用户摘要影响并确认。不要直接修改 `.sdd/`，不要展示 JSON、阶段码或内部路径。

$ARGUMENTS
