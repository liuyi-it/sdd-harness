import { type ImpactPayload, type McpCapabilities, type McpQueryInput, type McpQueryResult } from "./mcp-query.js";
/**
 * CodebaseAdapter 封装两类代码上下文来源：
 * 1. 优先使用 codebase-memory-mcp (pinned v0.8.1 / f0c9be1)
 * 2. 不可用时退回受限文件扫描
 *
 * 二期新增 V2 capability + 结构化 query 接口，所有跨 provider 一致性由本类负责。
 */
export interface CodebaseSummary {
    codebaseSummary: string;
    packageStructure: string;
    architecture: string;
}
export interface McpDiagnostics {
    installed: boolean;
    configured: boolean;
    connected: boolean;
    callable: boolean;
    indexed: boolean;
    officialUrl: string;
    message?: string;
}
export interface CodebaseResult extends CodebaseSummary {
    provider: "codebase-memory-mcp" | "fallback-file-scan";
    degraded: boolean;
    reason?: string;
    diagnostics: McpDiagnostics;
}
export declare const MCP_UNAVAILABLE_REASON = "codebase-memory-mcp unavailable";
export interface McpTransport {
    isAvailable(): Promise<boolean>;
    index(root: string): Promise<void>;
    summarize(root: string): Promise<CodebaseSummary>;
    inspect?(root: string): Promise<Partial<McpDiagnostics>>;
    /** V2: 返回 MCP 暴露的工具集合，缺失时按空集合处理并写入 partial。 */
    capabilities?(root: string): Promise<string[]>;
    /**
     * V2: 结构化查询；缺失时 Core 自动以 fallback-file-scan 返回
     * degraded=true 的同结构结果，禁止调用方自定义 payload shape。
     */
    query?(root: string, input: McpQueryInput): Promise<unknown>;
}
export declare class CodebaseAdapter {
    private readonly transport?;
    constructor(transport?: McpTransport | undefined);
    initialize(root: string): Promise<CodebaseResult>;
    /**
     * V2 capability discovery：返回 MCP 固定版本的工具清单；缺失时按 partial 写入，
     * 仍必须保留 officialUrl、version、commit 三项事实。
     */
    capabilities(): Promise<McpCapabilities>;
    /**
     * V2 query：唯一允许通过 Core 访问代码库上下文的入口。任何 transport.query
     * 返回必须命中 ImpactPayload / CodebaseSummary 等已知结构；其余 shape 一律降级。
     */
    query<TPayload = unknown>(input: McpQueryInput): Promise<McpQueryResult<TPayload>>;
    writeCapabilityArtifacts(root: string): Promise<{
        capabilitiesPath: string;
        diagnosticsPath: string;
    }>;
    queryImpact(root: string, input: McpQueryInput): Promise<McpQueryResult<ImpactPayload>>;
    private inspectDiagnostics;
    private fallback;
}
//# sourceMappingURL=codebase-adapter.d.ts.map