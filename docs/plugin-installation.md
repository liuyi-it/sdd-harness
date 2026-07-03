# 插件安装说明

## 前置要求

- macOS 或 Windows 上的 Node.js 20 及以上版本
- 项目内安装的 Claude Code 或 Codex 插件包
- 可选的 `codebase-memory-mcp v0.8.1`；当 MCP 不可用时会自动降级为受限文件扫描

安装目标插件包后，由宿主环境创建对应 Adapter，并注入宿主提供的 `TaskExecutor` 与可选的 `McpTransport`。随后在 Claude Code 中运行 `/sdd.init`，或在 Codex 中运行 `sdd init`。

重复执行 `init` 时，会保留用户手工修改过的配置和说明文件，同时补回缺失的生成文件。升级流程会检查 `schemaVersion`、备份并迁移 `.sdd/state.json`，同时生成 `.sdd/migration-report.md`。

当前 MVP 不提供自动卸载。手工清理分两层：

- **移除项目内集成文件**：删除 `.sdd/`、`.claude/commands/sdd.*`、`.claude/skills/sdd-harness/`、`.codex/skills/sdd-harness/`
- **移除宿主侧插件安装**：删除 Claude marketplace 安装项，或删除 Codex 本地插件目录 `~/.codex/plugins/sdd-harness` 及其 `~/.agents/plugins/marketplace.json` 条目
