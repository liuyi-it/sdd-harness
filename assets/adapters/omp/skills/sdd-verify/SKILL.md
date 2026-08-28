---
name: sdd-verify
description: 验证实现、进行代码审查或完成统一质量门禁时使用。
---

# SDD Verify

先运行 status；多任务且未指定时询问。运行 `sdd verify --change <id> --json`。收到修复行动时只处理报告阻断项，执行全部 verification 并用 `--result-json` 回传；一轮后仍失败必须询问用户，明确同意后才使用 `--continue`。
