---
name: sdd-verify
description: 用户要验证实现、进行代码审查或完成 SDD 质量门禁时使用。
---

# SDD Verify

先运行 `sdd status --json`；多任务且未指定时询问。运行 `sdd verify --change <id> --json`，统一检查规格覆盖、任务证据、实际文件范围、敏感信息和依赖计划。若返回 `AGENT_FIX_EXECUTION`，只修复报告中的阻断项，执行全部 verification 并用 `--result-json` 回传。首轮后仍失败时必须停下询问用户；只有用户明确同意才使用 `--continue`。
