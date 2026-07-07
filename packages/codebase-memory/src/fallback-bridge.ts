import type { McpQueryInput, McpQueryResult } from "./types.js";
import { degradedWarning } from "./warnings.js";

/**
 * fallback-file-scan 查询实现
 * 当 codebase-memory-mcp 不可用时使用
 */
export async function fallbackQuery(
  input: McpQueryInput,
): Promise<McpQueryResult> {
  const warning = degradedWarning("MCP 当前不可用");

  return {
    schemaVersion: "1.0.0",
    provider: "fallback-file-scan",
    mode: "fallback",
    degraded: true,
    intent: input.intent,
    query: input.query,
    items: [],
    warnings: [warning],
  };
}
