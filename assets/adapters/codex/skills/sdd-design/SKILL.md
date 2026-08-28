---
name: sdd-design
description: 用户要为已批准规格生成技术设计或继续 SDD 设计阶段时使用。
---

# SDD Design

先运行 `sdd status --json`；存在多个活动任务且未指定时询问。运行 `sdd design --change <id> --json`，根据规格、真实实现和 Context Pack 给出当前代码事实、推荐方案、关键取舍、接口数据流、错误处理、测试策略与风险，生成符合 resultSchema 的 JSON 后用 `--result-json` 回传。不得修改业务文件。
