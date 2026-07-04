# 插件安装说明

## 前置要求

- macOS 或 Windows 上的 Node.js 20 及以上版本
- 项目内安装的 Claude Code 或 Codex 插件包
- 可选的 `codebase-memory-mcp v0.8.1`；当 MCP 不可用时会自动降级为受限文件扫描

安装目标插件包后，由宿主环境创建对应 Adapter，并注入宿主提供的 `TaskExecutor` 与可选的 `McpTransport`。随后在 Claude Code 中运行 `/sdd.init`，或在 Codex 中运行 `sdd init`。

如果是自定义集成或测试环境，可直接构造适配器：

```ts
import { CodexAdapter } from "@sdd-harness/codex-plugin";
import { ClaudeCodeAdapter } from "@sdd-harness/claude-code-plugin";

const codexAdapter = new CodexAdapter({ taskExecutor, mcpTransport });
const claudeAdapter = new ClaudeCodeAdapter({ taskExecutor, mcpTransport });
```

其中：

- `taskExecutor` 为必填，由宿主负责真正执行 build 阶段任务
- `mcpTransport` 为可选，用于接入 `codebase-memory-mcp`
- 不提供 `mcpTransport` 时会自动退回 `fallback-file-scan`

重复执行 `init` 时，会保留用户手工修改过的配置和说明文件，同时补回缺失的生成文件。升级流程会检查 `schemaVersion`、先备份整份 `.sdd/` 到 `.sdd.migration.bak/`，并额外保留 `.sdd/state.json.migration.bak`，随后执行迁移并生成 `.sdd/migration-report.md`。

当前 MVP 不提供自动卸载。手工清理分两层：

- **移除项目内集成文件**：删除 `.sdd/`、`.claude/commands/sdd.*`、`.claude/skills/sdd-harness/`、`.codex/skills/sdd-harness/`
- **移除宿主侧插件安装**：删除 Claude marketplace 安装项，或删除 Codex 本地插件目录 `~/.codex/plugins/sdd-harness` 及其 `~/.agents/plugins/marketplace.json` 条目
