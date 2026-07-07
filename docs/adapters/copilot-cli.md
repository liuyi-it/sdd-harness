# GitHub Copilot CLI Adapter（文档级）

## 前置条件

1. 安装 sdd CLI（参考 README）
2. Node.js >= 22

## 使用方式

在 Copilot CLI 场景中：

```bash
# 初始化
sdd init

# 创建变更
sdd new "实现订单取消功能"

# 生成计划
sdd plan

# 获取下一个构建任务
sdd build next --json
```

## 当前能力限制

- 支持等级：Level 3/4（视工具能力而定）
- 可能不支持完整多文件编辑和 JSON 结果协议
- 建议配合其他 Agent 或手动完成 build 阶段
