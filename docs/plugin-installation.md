# 插件安装说明

## 前置要求

- macOS 或 Windows 上的 Node.js 20 及以上版本
- 项目内安装的 Claude Code 或 Codex 插件包
- 可选的 `codebase-memory-mcp v0.8.1`；当 MCP 不可用时会自动降级为受限文件扫描

安装目标插件包后，由宿主环境创建对应 Adapter，并注入宿主提供的 `TaskExecutor` 与可选的 `McpTransport`。随后在 Claude Code 中运行 `/sdd.init`，或在 Codex 中运行 `sdd init`。

重复执行 `init` 时，会保留用户手工修改过的配置和说明文件，同时补回缺失的生成文件。升级流程会备份并迁移 `.sdd/state.json`。当前 MVP 的卸载方式仍是手工删除：在保留所需归档内容后，移除 `.sdd/`、生成的 `.claude/commands/sdd.*`、`.claude/skills/sdd-harness` 与 `.codex/skills/sdd-harness`。
