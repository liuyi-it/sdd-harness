import type { CliWarning } from "@sdd-harness/core";

export const WARNING_CODES = {
  CODEBASE_MEMORY_UNAVAILABLE: "W_CODEBASE_MEMORY_UNAVAILABLE",
  CODEBASE_MEMORY_START_FAILED: "W_CODEBASE_MEMORY_START_FAILED",
  CODEBASE_MEMORY_INDEX_FAILED: "W_CODEBASE_MEMORY_INDEX_FAILED",
  CODEBASE_MEMORY_QUERY_TIMEOUT: "W_CODEBASE_MEMORY_QUERY_TIMEOUT",
  CODEBASE_MEMORY_PROVIDER_CRASHED: "W_CODEBASE_MEMORY_PROVIDER_CRASHED",
} as const;

/** 生成降级警告 — MCP 不可用时使用 */
export function degradedWarning(reason: string): CliWarning {
  return {
    code: WARNING_CODES.CODEBASE_MEMORY_UNAVAILABLE,
    message: `codebase-memory-mcp 不可用，已降级为 fallback-file-scan。${reason}`,
    next: "sdd codebase doctor",
  };
}

/** 生成 MCP 启动失败警告 */
export function startFailedWarning(detail: string): CliWarning {
  return {
    code: WARNING_CODES.CODEBASE_MEMORY_START_FAILED,
    message: `codebase-memory-mcp 启动失败: ${detail}`,
    next: "sdd codebase doctor",
  };
}
