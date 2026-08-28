---
name: sdd-harness
description: 用逐阶段 SDD 工作流处理软件实现、修复、重构和测试任务；协调规格、设计、计划、构建、质量验证与归档。
---

# SDD Harness

仅在软件变更任务或用户明确提到 SDD 时使用。普通问答和只读解释不启动工作流。

1. 所有状态读写只通过 `sdd` CLI，不直接修改 `.sdd/`。
2. 先运行 `sdd status --json`。若存在多个活动 change 且用户没有明确 change，列出简短标题并询问用户选择；不得自动选择最近任务。
3. 未初始化时运行 `sdd init --json`；新需求运行 `sdd new "<需求>" --json`。
4. 严格按 `new → design → plan → build → verify → archive` 推进，不存在 `auto` 或独立 `review`。
5. `AGENT_PHASE_EXECUTION`：基于 Context Pack 和真实代码生成符合 resultSchema 的 JSON；规格、设计、计划阶段不得修改业务文件，然后用对应命令的 `--result-json` 回传。
6. `AGENT_TASK_EXECUTION`：只修改 allowedFiles，按任务内部 steps 完成测试、实现和验证，执行全部 verification，再用 `sdd build complete` 回传结果。
7. `AGENT_FIX_EXECUTION`：只修复质量报告中的阻断问题，执行全部 verification，再用 `sdd verify --result-json` 回传；Core 自动控制一轮修复，后续轮次必须先询问用户。
8. 文档深度和任务数量随复杂度调整，但任何任务都必须有可验收规格；不要为小任务引入额外流程、角色或 subagent。
9. 对用户只汇报目标、必要问题、关键决策、改动、验证和风险；内部 JSON、Context Pack、标识符和运行路径不直接展示。
