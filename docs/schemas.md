# Schema 说明

正式 JSON Schema 位于仓库根目录 `schemas/`，Rust Core 在编译时内嵌并执行校验。当前保留六个事实模型：

| Schema | 对应数据 |
| --- | --- |
| `state.schema.json` | `.sdd/runtime.json` 的 `state` 节点 |
| `runtime.schema.json` | `.sdd/runtime.json` 顶层统一运行数据 |
| `task.schema.json` | runtime 计划中的任务定义 |
| `task-result.schema.json` | runtime 运行级任务结果 |
| `report.schema.json` | runtime 中的 verify/review 报告 |
| `artifact.schema.json` | runtime 中的制品条目 |

runtime 内的规格、设计、计划和归档模型沿用各自的 `schemaVersion` 字段。状态、runtime、任务、任务结果和报告会在关键读写边界执行结构校验（required/type/enum/pattern/minimum/uniqueItems/additionalProperties/$ref）；数组元素也按 `items` 递归校验。任务定义在读取计划时执行 `task.schema.json` 校验。


## 报告字段

`report.schema.json` 中 `issues[]` 的 `code`、`severity`、`message` 为必填，其余均为可选：`file`、`origin`、`category`、`startLine`、`endLine`、`existingCode`、`suggestionCode`。OCR 后端产生的 finding 带 `origin="ocr"`，并可选携带定位（`startLine`/`endLine`）、分类（`category`）和建议代码（`suggestionCode`）；原版确定性 finding 保持只有 `file` 的既有格式。报告同时以 `minimality.ocr.status` 记录 OCR 结果：`completed`、`not-found`、`unavailable`、`failed`、`invalid-output`、`off`、`skipped` 或 `blocked-by-deterministic-review`。

状态写入使用临时文件、同步、重命名和 `runtime.json.bak` 恢复，并维护主文件与备份各自的 SHA-256 校验和。缺失或不匹配的校验和、损坏或版本不匹配都会返回 `E_STATE_CORRUPTED`，不会猜测内容继续执行。

## 校验

```bash
cargo test -p sdd-core --test schema_validator --test state_store --test artifact_store
```
