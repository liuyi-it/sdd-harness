/** 单个制品在集中清单中的摘要信息。 */
export interface ArtifactMetadata {
    schemaVersion: "1.0.0";
    generatedBy: "sdd-harness";
    inputHash: string;
    artifactHash: string;
    createdAt: string;
}
/** 所有制品摘要集中写入 `.sdd/artifacts.json`，避免为每个文件生成 sidecar。 */
export declare class ArtifactWriter {
    private readonly renameFile;
    constructor(renameFile?: (source: string, target: string) => Promise<void>);
    writeGroupAtomically(artifacts: Array<{
        path: string;
        content: string;
        inputs: unknown;
    }>): Promise<void>;
    write(path: string, content: string, inputs: unknown): Promise<ArtifactMetadata>;
    metadata(path: string): Promise<ArtifactMetadata | undefined>;
    isUnmodified(path: string): Promise<boolean>;
    private writeMetadataEntries;
}
export declare function artifactInputHash(value: unknown): string;
//# sourceMappingURL=artifact-writer.d.ts.map