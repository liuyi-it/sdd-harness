# Schema 说明

正式 JSON Schema 位于仓库根目录 `schemas/`，`sdd init` 会将其安装到目标项目的 `.sdd/schemas/`。

| Schema                              | 对应数据                              |
| ----------------------------------- | ------------------------------------- |
| `config.schema.json`                | `.sdd/config.yml`，当前版本 `1.3.0`   |
| `state.schema.json`                 | `.sdd/state.json`，当前版本 `1.4.0`   |
| `task.schema.json`                  | `plan.json` 中的任务定义              |
| `artifact-metadata.schema.json`     | `.sdd/artifacts.json` 中单个制品摘要  |
| `task-execution-result.schema.json` | 运行级任务结果，版本 `1.2.0`          |
| `loop.schema.json`                  | auto Loop 规格                        |
| `loop-run.schema.json`              | auto 运行与步骤审计                   |
| `mcp-query-result.schema.json`      | MCP 或 fallback 查询结果              |
| `verify-report.schema.json`         | `verify-report.v1.2.json`             |
| `review-issue.schema.json`          | 确定性审查问题                        |
| `review-report.schema.json`         | `review-report.v2.json`，版本 `2.0.0` |

`spec.json`、`plan.json` 和 `archive.json` 使用各自的 `schemaVersion: "2.0.0"`，由 Core 在读取时执行结构校验。`plan.json.dependencies` 是可选依赖决策数组；`review-report.v2.json.minimality` 是可选的改动规模、依赖 delta 与债务记录。它们不兼容旧的展开式规格/计划目录。

Agent 结果的可选 `minimality` 字段记录复用、标准库/平台选择、依赖、抽象与债务声明。Core 不信任自由文本或 Agent 声明：依赖变化仍由 Git 快照中的 `package.json` 解析确定。

## 兼容边界

- 状态读取器会迁移受支持的旧状态，并保留 `state.json.migration.bak`。
- 不支持或损坏的状态/制品返回 `E_STATE_CORRUPTED`，不会猜测内容继续执行。
- TaskExecutor 可返回旧版结果，Core 在 build 阶段归一化为 1.2.0 运行级制品。
- 字符串命令只有在严格白名单内才能转换为 argv；包含 shell 语义时直接阻断。

## 校验

```bash
npm run validate:schemas
```

校验脚本运行真实小仓库流程，并使用实际产生的配置、状态、任务、集中制品摘要、任务结果、Loop、MCP 查询、verify 报告和 review 报告验证 Schema。
