/**
 * 二期 MCP 查询固定版本约束：仅允许 codebase-memory-mcp v0.9.0 (commit b637e33)。
 * 任何其它 MCP provider 都不进入 Core 路径，由 Core 经统一 V2 transport 进行查询或降级。
 */
export const MCP_PINNED_PROVIDER = "codebase-memory-mcp";
export const MCP_PINNED_VERSION = "0.9.0";
export const MCP_PINNED_COMMIT = "b637e33";
export const MCP_FALLBACK_PROVIDER = "fallback-file-scan";
export function isSupportedIntent(intent) {
    const supported = [
        "impact",
        "related-files",
        "symbols",
        "callers",
        "callees",
        "routes",
        "tests",
        "architecture",
    ];
    return supported.includes(intent);
}
export function createMcpQueryBuilder(now = () => new Date()) {
    return {
        capabilitiesFrom(tools) {
            return {
                provider: MCP_PINNED_PROVIDER,
                version: MCP_PINNED_VERSION,
                commit: MCP_PINNED_COMMIT,
                officialUrl: "https://github.com/DeusData/codebase-memory-mcp",
                availableTools: [...tools].sort(),
                supportedIntents: [
                    "impact",
                    "related-files",
                    "symbols",
                    "callers",
                    "callees",
                    "routes",
                    "tests",
                    "architecture",
                ],
                generatedAt: now().toISOString(),
            };
        },
        summarizeCodebase(summary) {
            return {
                schemaVersion: "1.2.0",
                intent: "architecture",
                provider: MCP_PINNED_PROVIDER,
                degraded: false,
                confidence: 0.85,
                generatedAt: now().toISOString(),
                payload: summary,
            };
        },
        buildImpactResult(input, payload) {
            return {
                schemaVersion: "1.2.0",
                intent: input.intent,
                provider: MCP_PINNED_PROVIDER,
                degraded: false,
                confidence: 0.8,
                generatedAt: now().toISOString(),
                payload: {
                    files: dedupe(payload.files),
                    symbols: dedupe(payload.symbols),
                    tests: dedupe(payload.tests),
                    risks: dedupe(payload.risks),
                },
            };
        },
        buildFallback(intent, reason, payload) {
            return {
                schemaVersion: "1.2.0",
                intent,
                provider: MCP_FALLBACK_PROVIDER,
                degraded: true,
                reason,
                confidence: 0.3,
                generatedAt: now().toISOString(),
                payload,
            };
        },
    };
}
function dedupe(values) {
    const seen = new Set();
    const ordered = [];
    for (const value of values) {
        if (seen.has(value))
            continue;
        seen.add(value);
        ordered.push(value);
    }
    return ordered;
}
/**
 * 标准降级原因。fallback 结果必须设置 reason = MCP_QUERY_UNAVAILABLE 或更具体描述。
 */
export const MCP_QUERY_UNAVAILABLE = "codebase-memory-mcp unavailable";
//# sourceMappingURL=mcp-query.js.map