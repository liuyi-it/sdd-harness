import type { CliWarning } from "@sdd-harness/core";

/** MCP 传输层 V3 接口 */
export interface McpTransportV3 {
  provider: "codebase-memory-mcp" | "fallback-file-scan";
  mode: "managed" | "external" | "fallback";
  inspect(root: string): Promise<McpDiagnostics>;
  capabilities(root: string): Promise<McpCapabilities>;
  start?(root: string): Promise<McpLifecycleResult>;
  stop?(root: string): Promise<McpLifecycleResult>;
  index(input: McpIndexInput): Promise<McpIndexResult>;
  query(input: McpQueryInput): Promise<McpQueryResult>;
}

export interface McpDiagnostics {
  schemaVersion: "1.0.0";
  requestedProvider: "codebase-memory-mcp";
  actualProvider: "codebase-memory-mcp" | "fallback-file-scan";
  mode: "managed" | "external" | "fallback";
  degraded: boolean;
  version?: string;
  lastStartedAt?: string;
  lastIndexedAt?: string;
  indexStatus: "READY" | "STALE" | "MISSING" | "FAILED";
  storageDir?: string;
  errors: McpDiagnosticError[];
}

export interface McpDiagnosticError {
  code: string;
  stage: string;
  message: string;
}

export interface McpCapabilities {
  schemaVersion: "1.0.0";
  provider: "codebase-memory-mcp" | "fallback-file-scan";
  availableTools: string[];
  supportedIntents: CodebaseQueryIntent[];
  supportsIndex: boolean;
  supportsGraphQuery: boolean;
}

export interface McpLifecycleResult {
  provider: "codebase-memory-mcp" | "fallback-file-scan";
  mode: "managed" | "external" | "fallback";
  status: "STARTED" | "ALREADY_RUNNING" | "STOPPED" | "UNAVAILABLE" | "FAILED";
  pid?: number;
  endpoint?: string;
  message?: string;
  /** 仅运行期使用，不序列化到 diagnostics。 */
  session?: McpSession;
  tools?: string[];
}

export interface McpSession {
  call(method: string, params?: Record<string, unknown>): Promise<unknown>;
  notify(method: string, params?: Record<string, unknown>): void;
  close(): void;
  isAlive(): boolean;
  onExit(listener: () => void): void;
}

export interface McpIndexInput {
  root: string;
  force?: boolean;
}

export interface McpIndexResult {
  schemaVersion: "1.0.0";
  provider: "codebase-memory-mcp" | "fallback-file-scan";
  status: "INDEXED" | "FAILED";
  fileCount?: number;
  warnings?: CliWarning[];
}

export interface McpQueryInput {
  query: string;
  intent: CodebaseQueryIntent;
  root: string;
}

export interface McpQueryResult {
  schemaVersion: "1.0.0";
  provider: "codebase-memory-mcp" | "fallback-file-scan";
  mode: "managed" | "external" | "fallback";
  degraded: boolean;
  intent: CodebaseQueryIntent;
  query: string;
  items: McpQueryItem[];
  warnings?: CliWarning[];
}

export interface McpQueryItem {
  type: "file" | "symbol" | "route" | "test" | "module" | "config" | "risk";
  path?: string;
  symbol?: string;
  confidence: number;
  reason: string;
}

export type CodebaseQueryIntent =
  | "impact"
  | "related-files"
  | "symbols"
  | "callers"
  | "callees"
  | "routes"
  | "tests"
  | "architecture"
  | "entrypoints"
  | "data-flow"
  | "config";

/** .sdd/config.yml 中 codebase 配置段 */
export interface CodebaseConfig {
  provider: "codebase-memory-mcp";
  mode: "managed" | "external" | "fallback";
  version: string;
  autoStart: boolean;
  autoIndex: boolean;
  requireAvailable: boolean;
  storageDir: string;
  diagnosticsFile: string;
  capabilitiesFile: string;
  timeoutMs: number;
  fallback: {
    enabled: boolean;
    provider: "fallback-file-scan";
  };
}
