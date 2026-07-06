# Schema 说明

正式 JSON Schema 位于仓库根目录的 `schemas/`，并由 `init` 安装到 `.sdd/schemas/`。二期 A 起，新增状态、Loop、项目规范和运行级任务结果统一固定为 `schemaVersion: "1.2.0"`。

- `config.schema.json`：项目、插件、代码库、流程、质量和安全配置。
- `state.schema.json`：带版本的工作流状态，以及任务/制品状态。
- `task.schema.json`：关联需求编号的任务范围与验证契约。
- `artifact-metadata.schema.json`：源输入摘要和生成制品摘要。
- `task-execution-result.schema.json`：运行级单任务结果制品，使用结构化命令证据和 `fileDelta`。
- `loop.schema.json` / `loop-run.schema.json`：`auto` Loop 规格、运行状态与步骤审计。

当读取到旧版状态文件时，系统会迁移到 `1.2.0`，并保留 `state.json.migration.bak`。对不支持或损坏的状态文件，系统会返回 `E_STATE_CORRUPTED`，而不是猜测状态继续执行。

兼容策略如下：

- 运行期 `TaskExecutor` 仍可返回 v1 结果（`modifiedFiles` / `tddEvidence` / `verification`）。
- Core 会在 `build` 阶段把 v1 结果安全归一化为 1.2.0 运行级制品。
- 字符串命令只允许在严格白名单下拆分为普通 argv；出现管道、重定向、命令替换等 shell 语义时直接返回 `E_SECURITY_BLOCKED`。
