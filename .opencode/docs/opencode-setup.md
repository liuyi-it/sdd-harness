# OpenCode 接入指南

## 安装 sdd CLI

参考仓库 README：`git clone` + `bash scripts/install.sh`

## 配置 OpenCode 原生接入

在 OpenCode 中使用 `/sdd-init`，会自动识别当前宿主并写入 `.opencode/skills/`、`.opencode/commands/` 和 `.opencode/agents/`；不要让用户填写 Agent 参数。

## 使用

在 OpenCode 对话中提供完整需求，或使用 `/sdd`、`/sdd-new` 等命令。OpenCode 会按 Skill 调用 `sdd auto "<需求>" --json` 并遵循 Agent Task Protocol。需求存在阻塞问题时先提问；不应默认使用 `--non-interactive`，否则未回答的阻塞问题会直接失败。
