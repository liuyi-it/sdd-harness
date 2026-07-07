import type { CodebaseSummary } from "./codebase-adapter.js";

/**
 * 二期 MCP 查询固定版本约束：仅允许 codebase-memory-mcp v0.8.1 (commit f0c9be1)。
 * 任何其它 MCP provider 都不进入 Core 路径，由 Core 经统一 V2 transport 进行查询或降级。
 */
export const MCP_PINNED_PROVIDER = "codebase-memory-mcp";
export const MCP_PINNED_VERSION = "0.8.1";
export const MCP_PINNED_COMMIT = "f0c9be1";
export const MCP_FALLBACK_PROVIDER = "fallback-file-scan";

/**
 * intent 决定查询的返回结构。Core 不允许调用方自定义 payload，避免任意 payload
 * 通过未审计通道反推执行。
 */
export type McpQueryIntent =
  | "impact"
  | "related-files"
  | "symbols"
  | "callers"
  | "callees"
  | "routes"
  | "tests"
  | "architecture";

export interface McpQueryInput {
  intent: McpQueryIntent;
  changeId?: string;
  requirement?: string;
  hint?: { file?: string; symbol?: string; path?: string };
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
  capabilitiesFrom(tools: string[]): McpCapabilities;
  summarizeCodebase(summary: CodebaseSummary): McpQueryResult<CodebaseSummary>;
  buildImpactResult(
    input: McpQueryInput,
    payload: ImpactPayload,
  ): McpQueryResult<ImpactPayload>;
  buildFallback<T>(
    intent: McpQueryIntent,
    reason: string,
    payload: T,
  ): McpQueryResult<T>;
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

export function isSupportedIntent(intent: string): intent is McpQueryIntent {
  const supported: readonly McpQueryIntent[] = [
    "impact",
    "related-files",
    "symbols",
    "callers",
    "callees",
    "routes",
    "tests",
    "architecture",
  ];
  return supported.includes(intent as McpQueryIntent);
}

export function createMcpQueryBuilder(
  now: () => Date = () => new Date(),
): McpQueryBuilder {
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

function dedupe(values: readonly string[]): string[] {
  const seen = new Set<string>();
  const ordered: string[] = [];
  for (const value of values) {
    if (seen.has(value)) continue;
    seen.add(value);
    ordered.push(value);
  }
  return ordered;
}

/**
 * 标准降级原因。fallback 结果必须设置 reason = MCP_QUERY_UNAVAILABLE 或更具体描述。
 */
export const MCP_QUERY_UNAVAILABLE = "codebase-memory-mcp unavailable";
