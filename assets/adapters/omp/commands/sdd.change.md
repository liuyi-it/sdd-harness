---
description: 修订已有 SDD 需求并同步变更文档
---

使用 `sdd change <change-id> <新需求>` 修改当前活动且未归档的变更。先确认完整的新需求；不要直接编辑 `.sdd/`。成功后确认当前规格和提案已更新、旧的 design/plan/tasks 已清除，再按 `next: sdd design` 重新生成设计与计划。不要查找或生成需求级 revision、backup、diff、snapshot 文件；runtime 的崩溃恢复文件不属于需求历史，Git 负责需求历史。

$ARGUMENTS
