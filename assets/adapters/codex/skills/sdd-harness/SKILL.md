---
name: sdd-harness
description: 在软件实现、修复、重构、测试或代码审查任务中使用 SDD 工作流；对可独立的探索、实现和审查任务按边界派发 Codex subagent。
---

# SDD Harness

仅在软件变更任务或明确提到 SDD 时使用；普通问答不启动工作流。

## 工作原则

- 通过 `sdd` CLI 推进所有 SDD 阶段，不从源码入口绕过状态机，也不直接修改 `.sdd/` 内部文件。
- 首次 `sdd new` 或 `sdd auto` 必须携带非空需求；遇到 `CLARIFYING` 时只询问当前最关键的阻塞问题，并使用 `--answers` 继续。
- 对用户只汇报目标、变更、验证、风险和下一步；CLI JSON、任务 ID、Context Pack、状态码和内部路径只在内部处理。
- 需求、架构、安全边界、不可逆外部操作和最终验收由主 Agent 决定；subagent 不能自行扩大任务范围或替代最终验收。

## 默认流程

1. 先确认任务类型与范围；只读分析、方案或实现请求分别停在相应阶段。
2. 未初始化时运行 `sdd init --json`。当前 Codex 是默认宿主，不需要交互选择。
3. 用 `sdd auto "<需求>" --json` 推进工作流；高风险变更在设计/计划完成后先向用户说明影响并等待确认。
4. `AGENT_TASK_EXECUTION` 出现时，读取 Context Pack，只修改 `allowedFiles`，执行指定 verification，并用 `sdd build complete` 提交结果。
5. 所有任务完成后运行 `sdd verify` 与 `sdd review`；只有两者通过才能归档。

## Subagent 编排

Codex 的主 Agent 负责拆分、等待、汇总和最终检查；优先将嘈杂的只读工作交给 subagent，避免主线程被原始日志和搜索结果淹没。

- `sdd-explorer`：只读梳理调用链、状态转换、约束和可验证证据。适用于复杂需求开始前、跨模块定位和设计前的事实收集。
- `sdd-worker`：只执行一个边界明确的 SDD build 任务。主 Agent 必须提供任务目标、允许文件、验收条件和验证命令；多个 worker 仅在文件集合和副作用完全不重叠时并行。
- `sdd-reviewer`：只读复核 diff、真实调用路径、回归风险和测试缺口。它不修改代码，主 Agent 根据证据决定是否修复。

对独立的探索、测试日志分析和审查可并行派发并等待全部结果；不要并行编辑共享文件或 `.sdd/` 状态。每个 subagent 的返回必须包含：结论、证据位置、涉及文件、验证结果和未决风险。主 Agent 必须重新检查 diff、文件范围和验证输出，不能直接相信 subagent 的结论。

## 输出边界

不要向用户暴露 `.sdd/runtime.json`、策略包、Context Pack、任务/运行标识、错误码或调试字段，除非用户明确要求排障原始信息。
