import { SddError } from "../errors.js";
export const CURRENT_SCHEMA_VERSION = "1.4.0";
export const CURRENT_CONFIG_SCHEMA_VERSION = "1.3.0";
export const LEGACY_SCHEMA_VERSION = "1.0.0";
export function migrateWorkflowState(raw) {
    if (raw.schemaVersion !== LEGACY_SCHEMA_VERSION) {
        throw new SddError("E_STATE_CORRUPTED", `不支持的 state schemaVersion：${String(raw.schemaVersion ?? "unknown")}`, "sdd status");
    }
    return {
        from: LEGACY_SCHEMA_VERSION,
        to: CURRENT_SCHEMA_VERSION,
        state: {
            ...raw,
            schemaVersion: CURRENT_SCHEMA_VERSION,
            version: typeof raw.version === "number" ? raw.version + 1 : 1,
            updatedAt: new Date().toISOString(),
            activeLoop: null,
        },
        backupPaths: [".sdd/state.json.migration.bak", ".sdd.migration.bak"],
    };
}
export function migrateConfigDocument(raw) {
    if (raw.schemaVersion === CURRENT_CONFIG_SCHEMA_VERSION)
        return raw;
    if (raw.schemaVersion !== LEGACY_SCHEMA_VERSION) {
        throw new SddError("E_STATE_CORRUPTED", `不支持的 config schemaVersion：${String(raw.schemaVersion ?? "unknown")}`, "sdd init");
    }
    return {
        ...raw,
        schemaVersion: CURRENT_CONFIG_SCHEMA_VERSION,
    };
}
//# sourceMappingURL=schema-migration.js.map