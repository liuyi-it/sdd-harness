# 架构说明

```text
Claude Code 命令 ─┐
                  ├─ HostAdapter ─ Core ─ State / Artifacts / Git / MCP
Codex Skill ──────┘                 ├─ SpecEngine
                                   ├─ TddEngine
                                   └─ Quality Gates
```

`@sdd-harness/core` 是唯一允许修改工作流状态的组件。平台适配器只负责解析宿主命令格式，并原样返回 Core 输出的 `CommandResult`。宿主环境通过依赖注入提供 `McpTransport` 和 `TaskExecutor`，这样外部工具调用边界清晰、便于测试。

Core 会把所有流程事实写入 `.sdd/`。每个 Markdown 制品都配套元数据文件，记录输入摘要与制品 SHA-256。状态文件更新采用临时文件写入、落盘、重命名加备份恢复策略；所有写命令都必须先获取 `.sdd/lock`。
