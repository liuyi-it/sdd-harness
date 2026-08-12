---
name: sdd-harness
description: 用户要实现、修复、重构、测试或审查代码时使用；后台维护 SDD 状态，必要时把简单独立任务交给低成本 subagent。
---

# SDD Harness

只在软件变更任务或明确的 `/sdd.<command>` 调用时触发；普通问答不启动工作流。

## 默认入口

直接描述需求时：

1. 确认项目已初始化；未初始化时运行 `sdd init`。
2. 用 `sdd auto "<需求>" --json` 推进；JSON 只供自己读取，不展示给用户。
3. 遇到 `CLARIFYING` 按 `round` 只提出当前 frontier 问题，优先让用户明确目标、范围和验收，再继续追问角色、接口、前置条件和失败路径；收到答案后继续。

## 显式命令

`/sdd.<command>` 命令模板已经指定精确 CLI 命令。执行显式命令时只执行对应命令，不要把命令名当成新需求重新调用 `sdd auto`。仍然隐藏 JSON、阶段码、Context Pack、Policy Bundle 和内部路径。

遇到任务边界时读取 `contextPack`，只改 `allowedFiles`，按 `verification` 验证，并提交 `TaskExecutionResult`。

任务小、独立、低风险时，先看 `task` 工具给出的可用 Agent；优先使用 `sdd-worker`。它按 `@smol` → `@task` 解析当前可用模型，不要写死模型名。复杂、共享文件、架构、安全或外部副作用任务留给主 Agent。

subagent 完成后，主 Agent 必须重新检查 diff、文件范围和验证结果；不直接相信子 agent 的结论。所有任务完成后执行 `sdd verify`、`sdd review`，通过后归档。

只向用户汇报目标、变更、验证、风险和下一步；不要要求用户阅读 `.sdd`、阶段码、Context Pack 或内部 JSON。Core 是唯一事实源，不直接修改 `.sdd/runtime.json`。
