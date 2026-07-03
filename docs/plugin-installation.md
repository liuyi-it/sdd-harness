# Plugin Installation

## Requirements

- Node.js 20 or newer on macOS or Windows
- A project-local installation of the Claude Code or Codex plugin package
- Optional codebase-memory-mcp v0.8.1; unavailable MCP falls back to a bounded file scan

Install the desired package and instantiate its Adapter with the host's `TaskExecutor` and optional `McpTransport`. Run `/sdd.init` in Claude Code or `sdd init` in Codex.

Repeated init preserves user-edited config and instruction files while repairing missing generated files. Upgrades back up and migrate `.sdd/state.json`. MVP uninstall is manual: remove `.sdd/`, generated `.claude/commands/sdd.*`, `.claude/skills/sdd-harness`, and `.codex/skills/sdd-harness` after retaining any desired archive.
