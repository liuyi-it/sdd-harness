import { SddError } from "../errors.js";

export const CURRENT_SCHEMA_VERSION = "1.2.0";
export const LEGACY_SCHEMA_VERSION = "1.0.0";

export interface MigrationResult {
  from: "1.0.0";
  to: "1.2.0";
  state: Record<string, unknown>;
  backupPaths: string[];
}

export function migrateWorkflowState(
  raw: Record<string, unknown>,
): MigrationResult {
  if (raw.schemaVersion !== LEGACY_SCHEMA_VERSION) {
    throw new SddError(
      "E_STATE_CORRUPTED",
      `不支持的 state schemaVersion：${String(raw.schemaVersion ?? "unknown")}`,
      "sdd status",
    );
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

export function migrateConfigDocument(
  raw: Record<string, unknown>,
): Record<string, unknown> {
  if (raw.schemaVersion === CURRENT_SCHEMA_VERSION) return raw;
  if (raw.schemaVersion !== LEGACY_SCHEMA_VERSION) {
    throw new SddError(
      "E_STATE_CORRUPTED",
      `不支持的 config schemaVersion：${String(raw.schemaVersion ?? "unknown")}`,
      "sdd init",
    );
  }
  return {
    ...raw,
    schemaVersion: CURRENT_SCHEMA_VERSION,
  };
}
