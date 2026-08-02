# Schema 说明

正式 JSON Schema 位于仓库根目录 `schemas/`，Rust Core 在编译时内嵌并执行校验。当前只保留五个事实模型：

| Schema | 对应数据 |
| --- | --- |
| `state.schema.json` | `.sdd/state.json` |
| `task.schema.json` | `plan.json` 中的任务定义 |
| `task-result.schema.json` | 运行级任务结果 |
| `report.schema.json` | verify/review 报告 |
| `artifact.schema.json` | `.sdd/artifacts.json` 中的制品条目 |

`spec.json`、`plan.json` 与 `archive.json` 使用 `schemaVersion: "2.0.0"`。状态、任务结果和报告会在关键读写边界执行结构校验；数组元素也按 `items` 递归校验。

状态写入使用临时文件、同步、重命名和 `state.json.bak` 恢复。损坏或版本不兼容时返回 `E_STATE_CORRUPTED`，不会猜测内容继续执行。

## 校验

```bash
cargo test -p sdd-core --test schema_validator --test state_store --test artifact_store
```
