---
name: sdd-design
description: 为已批准规格生成技术设计或继续设计阶段时使用。
---

# SDD Design

先运行 status；存在多个活动任务且未指定时询问。运行 `sdd design --change <id> --json`，基于真实代码生成包含事实、决策取舍、接口数据流、错误处理、测试和风险的 resultSchema JSON，再用 `--result-json` 回传。不得修改业务文件。
