# Kimi Code Adapter（文档级）

## 前置条件

1. 安装 sdd CLI（参考 README）
2. Node.js >= 22

## 使用方式

在 Kimi Code 对话中，请求 SDD 工作流：

```
请执行 SDD 工作流：实现订单取消功能

使用以下步骤：
1. 运行 `sdd auto "实现订单取消功能" --json` 获取当前任务
2. 如果返回 AGENT_TASK_EXECUTION：读取 contextPack、修改 allowedFiles、运行 verification、写 TaskExecutionResult、运行 sdd build complete
3. 不修改 .sdd/state.json
```

## 当前能力限制

- 支持等级：Level 3/4（视工具能力而定）
- 如果不支持 JSON 结果协议，建议先执行 `sdd new` + `sdd plan` 生成 Context Pack，再手动推进 build
