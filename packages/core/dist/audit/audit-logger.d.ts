export interface AuditEvent {
    command: string;
    phase: string;
    result: "PASS" | "FAIL" | "PAUSED";
    message?: string;
    changeId?: string;
    /** 其它元数据；任意结构都会被脱敏。 */
    meta?: Record<string, unknown>;
}
interface AuditOptions {
    maxBytes?: number;
    retainedFiles?: number;
}
export declare class AuditLogger {
    private readonly path;
    private readonly maxBytes;
    private readonly retainedFiles;
    constructor(root: string, options?: AuditOptions);
    write(event: AuditEvent): Promise<void>;
    entries(): Promise<AuditEvent[]>;
    /**
     * 扫描指定文件并把命中写入审计；原始值不会被落盘。
     * 返回是否发现 private-key / github-token 等不可豁免命中。
     */
    reportFindings(file: string, contents: string): Promise<boolean>;
    private rotateIfRequired;
}
/**
 * 递归对 JSON 节点进行脱敏。命中敏感键或原始值包含常见 secret pattern 都用 [REDACTED] 替代。
 */
export declare function redactJson(value: unknown): unknown;
export declare function redactText(value: string): string;
export {};
//# sourceMappingURL=audit-logger.d.ts.map