export declare const CURRENT_SCHEMA_VERSION = "1.3.0";
export declare const LEGACY_SCHEMA_VERSION = "1.0.0";
export interface MigrationResult {
    from: "1.0.0";
    to: "1.3.0";
    state: Record<string, unknown>;
    backupPaths: string[];
}
export declare function migrateWorkflowState(raw: Record<string, unknown>): MigrationResult;
export declare function migrateConfigDocument(raw: Record<string, unknown>): Record<string, unknown>;
//# sourceMappingURL=schema-migration.d.ts.map