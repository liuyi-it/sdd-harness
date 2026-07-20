# OpenCode 接入指南

## 安装 sdd CLI

参考仓库 README：`git clone` + `bash scripts/install.sh`

## 配置 OpenCode 规则

将 `packages/opencode-adapter/rules/sdd-harness.md` 复制到 OpenCode 规则目录。

## 使用

在 OpenCode 对话中提供完整需求即可。OpenCode 会按规则调用 `sdd auto "<需求>" --json` 并遵循 Agent Task Protocol。需求存在阻塞问题时，OpenCode 会先提问；不应默认使用 `--non-interactive`，否则未回答的阻塞问题会直接失败。
