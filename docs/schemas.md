# Schemas

Canonical JSON Schemas live under `schemas/` and are installed into `.sdd/schemas/` by init.

- `config.schema.json`: project, plugin, codebase, workflow, quality, and security configuration.
- `state.schema.json`: versioned workflow state and task/artifact statuses.
- `task.schema.json`: requirement-linked task scope and verification contract.
- `artifact-metadata.schema.json`: source input and generated artifact hashes.

State schema version 0.9.0 migrates to 1.0.0 with `state.json.migration.bak`. Unsupported or corrupt state returns `E_STATE_CORRUPTED` rather than guessing.
