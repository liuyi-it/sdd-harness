# JSON Schema

所有 Schema 位于 `schemas/` 并编译进二进制。Core 的轻量校验器支持当前文件实际使用的 type、required、properties、additionalProperties、items、enum、const、pattern、minimum、minLength、minItems、uniqueItems、oneOf、anyOf 和同文档 `$ref`。

| Schema | 用途 |
| --- | --- |
| `runtime.schema.json` | Runtime 根结构，当前 schemaVersion 6 |
| `state.schema.json` | 项目初始化与索引状态，当前 schemaVersion 4 |
| `config.schema.json` | 宿主、Git 隔离、Context Pack 和审计限制，当前 schemaVersion 4 |
| `artifact.schema.json` | 制品类型、路径、输入和内容哈希 |
| `spec-result.schema.json` | Agent 回传的规格阶段结果 |
| `spec.schema.json` | Core 持久化的 READY 规格模型 |
| `design-result.schema.json` | Agent 回传的技术设计 |
| `plan-result.schema.json` | Agent 回传的计划与任务集合 |
| `task.schema.json` | 单个纵向任务 |
| `task-result.schema.json` | build 的任务执行结果 |
| `fix-result.schema.json` | verify 的受控修复结果 |
| `report.schema.json` | kind=quality 的统一质量报告 |

## 阶段结果

规格结果必须包含目标、included/excluded 范围、约束和 Requirement/Scenario 模型。设计结果必须包含真实代码现状、决策与理由、影响文件、接口、错误处理、测试、风险和回滚。计划结果必须包含全局约束、依赖决策和至少一个 task。

Core 对 result JSON 先做 Schema 校验，再反序列化为强类型，并递归拒绝占位内容。计划还会逐个用 task schema 复验，检查文件范围矛盾、验证命令、依赖环和规格覆盖。

## 纵向任务

taskId 格式为 `TASK-001`。`executionMode` 为 `TDD` 或 `VERIFY_ONLY`。TDD 的 steps 至少包含 TEST、IMPLEMENT、VERIFY；VERIFY_ONLY 至少包含 VERIFY。`interfaces`、用户可见结果、验收标准和 testSeam 用于防止计划退化成模糊动作列表。

## 结果传输

阶段、任务和修复行动都使用 `resultTransport=inline-json`，不接受 Agent 提供的任意结果文件路径。任务结果包含 evidence、verification 和 filesChanged；修复结果包含 fixId、status、verification 和 filesChanged。

## 报告

统一报告 `kind` 固定为 `quality`。issues 的必填字段为 code、severity、message；可选 file、category、startLine、endLine、existingCode、suggestionCode、origin。minimality 保存 Git 指纹和实际变更文件等可复核事实。
