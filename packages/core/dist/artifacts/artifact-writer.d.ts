/**
 * ArtifactWriter 统一负责阶段制品和元数据落盘。
 * 这样各命令只需要关心内容本身，不必重复实现摘要与元数据逻辑。
 */
export interface ArtifactMetadata {
    schemaVersion: "1.0.0";
    generatedBy: "sdd-harness";
    inputHash: string;
    artifactHash: string;
    createdAt: string;
}
export declare class ArtifactWriter {
    private readonly renameFile;
    constructor(renameFile?: (source: string, target: string) => Promise<void>);
    writeGroupAtomically(artifacts: Array<{
        path: string;
        content: string;
        inputs: unknown;
    }>): Promise<void>;
    write(path: string, content: string, inputs: unknown): Promise<ArtifactMetadata>;
    isUnmodified(path: string): Promise<boolean>;
}
export declare function artifactInputHash(value: unknown): string;
//# sourceMappingURL=artifact-writer.d.ts.map