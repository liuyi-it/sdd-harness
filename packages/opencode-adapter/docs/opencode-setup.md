# OpenCode 接入指南

## 安装 sdd CLI

参考仓库 README：`git clone` + `bash scripts/install.sh`

## 配置 OpenCode 规则

将 `packages/opencode-adapter/rules/sdd-harness.md` 复制到 OpenCode 规则目录。

## 使用

在 OpenCode 对话中请求 SDD 变更即可。OpenCode 会按规则调用 `sdd auto --json` 并遵循 Agent Task Protocol。
