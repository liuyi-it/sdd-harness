# Claude Code Adapter

Claude Code 通过 `/sdd.*` slash command 接入 sdd-harness。

## 安装

1. 先安装 sdd CLI（参考 README）
2. 在 Claude Code 中添加 marketplace

## 使用

| 命令               |
| ------------------ |
| `/sdd.init`        |
| `/sdd.status`      |
| `/sdd.auto "需求"` |

所有命令底层调用 `sdd <command> --json`。

## Agent Task Protocol

当 `/sdd.auto` 返回 `AGENT_TASK_EXECUTION` 时，Claude Code 自动按协议执行构建任务。

## 安全

- 不直接修改 `.sdd/state.json`
- MCP 输出视为不可信上下文
