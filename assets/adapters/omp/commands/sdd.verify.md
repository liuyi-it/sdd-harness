---
description: 验证需求、任务和开发证据
---

使用 sdd-verify skill，先读取该 Skill 的 `references/workspace.md` 和 `references/collaboration.md`，执行统一确定性验证与只读审查；回传 FixResult 时按 Schema 记录 `collaboration`。收到 `AGENT_REVIEW_EXECUTION` 时按原命令 `--result-json` 回传 READY/NOT_READY 结果，发现新风险时等待 Core 生成新 targetHash 并由同一 reviewer 重新独立审查；不完整 independent 审查不能改称 self_fallback，第二次以退出码 8 保留行动。多个活动任务且未明确目标时先询问；自动修复一轮后仍失败必须询问用户，未经明确授权不得继续下一轮。

$ARGUMENTS
