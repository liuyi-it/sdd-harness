# Codex Adapter

Codex 通过 Skill 机制接入 sdd-harness。

## 安装

1. 先安装 sdd CLI（参考 README）
2. 安装 Codex adapter 包

## 使用

```text
sdd init
sdd status
sdd auto "需求"
```

所有命令底层调用 `sdd <command> --json`。

## Agent Task Protocol

当 `sdd auto` 返回 `AGENT_TASK_EXECUTION` 时，Codex 按协议执行构建任务。
