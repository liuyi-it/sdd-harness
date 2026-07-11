import type { CodebaseSummary } from "./codebase-adapter.js";
/**
 * 二期 MCP 查询固定版本约束：仅允许 codebase-memory-mcp v0.9.0 (commit b637e33)。
 * 任何其它 MCP provider 都不进入 Core 路径，由 Core 经统一 V2 transport 进行查询或降级。
 */
export declare const MCP_PINNED_PROVIDER = "codebase-memory-mcp";
export declare const MCP_PINNED_VERSION = "0.9.0";
export declare const MCP_PINNED_COMMIT = "b637e33";
export declare const MCP_FALLBACK_PROVIDER = "fallback-file-scan";
/**
 * intent 决定查询的返回结构。Core 不允许调用方自定义 payload，避免任意 payload
 * 通过未审计通道反推执行。
 */
export type McpQueryIntent = "impact" | "related-files" | "symbols" | "callers" | "callees" | "routes" | "tests" | "architecture";
export interface McpQueryInput {
    intent: McpQueryIntent;
    query: string;
    changeId?: string;
    requirement?: string;
    hint?: {
        file?: string;
        symbol?: string;
        path?: string;
    };
}
export interface McpQueryResult<TPayload = unknown> {
    schemaVersion: "1.2.0";
    intent: McpQueryIntent;
    provider: typeof MCP_PINNED_PROVIDER | typeof MCP_FALLBACK_PROVIDER;
    degraded: boolean;
    /** 降级时必填，固定错误码；精确返回时省略。 */
    reason?: string;
    /** 0..1，精确结果 ≥ 0.6，fallback 严格 ≤ 0.45。 */
    confidence: number;
    generatedAt: string;
    payload: TPayload;
}
/**
 * capabilities 是从固定版本 MCP 中检索出的元数据，初始化时落盘便于审计。
 * 任何字段缺失都视为 capability partial。
 */
export interface McpCapabilities {
    provider: typeof MCP_PINNED_PROVIDER;
    version: string;
    commit: string;
    officialUrl: string;
    availableTools: string[];
    supportedIntents: readonly McpQueryIntent[];
    generatedAt: string;
}
export interface McpQueryBuilder {
    capabilitiesFrom(tools: string[], supportedIntents?: readonly McpQueryIntent[]): McpCapabilities;
    summarizeCodebase(summary: CodebaseSummary): McpQueryResult<CodebaseSummary>;
    buildImpactResult(input: McpQueryInput, payload: ImpactPayload): McpQueryResult<ImpactPayload>;
    buildFallback<T>(intent: McpQueryIntent, reason: string, payload: T): McpQueryResult<T>;
}
/**
 * ImpactPayload 是 intent=impact 时 payload 的稳定结构。Core 把它追加到 impact.md，
 * 同时只读取 stable 字段，不信任非枚举键。
 */
export interface ImpactPayload {
    files: string[];
    symbols: string[];
    tests: string[];
    risks: string[];
}
export declare function isSupportedIntent(intent: string): intent is McpQueryIntent;
export declare function createMcpQueryBuilder(now?: () => Date): McpQueryBuilder;
/**
 * 标准降级原因。fallback 结果必须设置 reason = MCP_QUERY_UNAVAILABLE 或更具体描述。
 */
export declare const MCP_QUERY_UNAVAILABLE = "codebase-memory-mcp unavailable";
//# sourceMappingURL=mcp-query.d.ts.map