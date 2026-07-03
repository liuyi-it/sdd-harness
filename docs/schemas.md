# Schema 说明

正式 JSON Schema 位于仓库根目录的 `schemas/`，并由 `init` 安装到 `.sdd/schemas/`。

- `config.schema.json`：项目、插件、代码库、流程、质量和安全配置。
- `state.schema.json`：带版本的工作流状态，以及任务/制品状态。
- `task.schema.json`：关联需求编号的任务范围与验证契约。
- `artifact-metadata.schema.json`：源输入摘要和生成制品摘要。

当读取到 `0.9.0` 版状态文件时，系统会迁移到 `1.0.0`，并保留 `state.json.migration.bak`。对不支持或损坏的状态文件，系统会返回 `E_STATE_CORRUPTED`，而不是猜测状态继续执行。
