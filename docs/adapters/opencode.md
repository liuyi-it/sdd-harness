# OpenCode Adapter

OpenCode 通过规则文件接入 sdd-harness。

## 安装

1. 先安装 sdd CLI（参考 README）
2. 将 `packages/opencode-adapter/rules/sdd-harness.md` 复制到 OpenCode 规则目录

## 使用

在 OpenCode 对话中直接请求 SDD 变更：

```text
请执行 SDD 工作流：实现订单取消功能
```

OpenCode 会自动按规则调用 `sdd auto --json`。
