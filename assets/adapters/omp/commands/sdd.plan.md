---
description: 生成计划和可勾选任务
---

使用 sdd-harness skill 执行 `sdd plan --json`。确认 `plan.md` 和可勾选 `tasks.md` 已生成；向用户用简短中文汇报方案、实施顺序、影响、验证方式和风险。若计划涉及公开接口、数据迁移、权限安全、外部服务、删除覆盖或新增依赖，先等待用户确认再进入构建。

$ARGUMENTS
