import fs from "node:fs/promises";
import path from "node:path";
import type { McpDiagnostics, McpDiagnosticError } from "./types.js";

/** 写入 diagnostics.json */
export async function writeDiagnostics(
  root: string,
  diagnosticsFile: string,
  diagnostics: McpDiagnostics,
): Promise<void> {
  const filePath = path.join(root, diagnosticsFile);
  await fs.mkdir(path.dirname(filePath), { recursive: true });
  await fs.writeFile(filePath, JSON.stringify(diagnostics, null, 2), "utf-8");
}

/** 创建默认 diagnostics 对象 */
export function createDiagnostics(
  overrides: Partial<McpDiagnostics> = {},
): McpDiagnostics {
  return {
    schemaVersion: "1.0.0",
    requestedProvider: "codebase-memory-mcp",
    actualProvider: "codebase-memory-mcp",
    mode: "managed",
    degraded: false,
    indexStatus: "READY",
    errors: [],
    ...overrides,
  };
}

/** 创建诊断错误条目 */
export function createDiagError(
  code: string,
  stage: string,
  message: string,
): McpDiagnosticError {
  return { code, stage, message };
}
