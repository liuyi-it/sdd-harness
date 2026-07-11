import type { CliWarning } from "@sdd-harness/core";
import { startManagedMcp, stopManagedMcp } from "./lifecycle.js";
import {
  createDiagnostics,
  createDiagError,
  writeDiagnostics,
} from "./diagnostics.js";
import { fallbackQuery } from "./fallback-bridge.js";
import { degradedWarning } from "./warnings.js";
import type {
  CodebaseConfig,
  McpDiagnostics,
  McpLifecycleResult,
  McpQueryInput,
  McpQueryResult,
  McpCapabilities,
} from "./types.js";
import { basename } from "node:path";

const DEFAULT_CONFIG: CodebaseConfig = {
  provider: "codebase-memory-mcp",
  mode: "managed",
  // 与 Core 的 pinned dependency 统一；npm 包名不接受前导 v。
  version: "0.9.0",
  autoStart: true,
  autoIndex: true,
  requireAvailable: false,
  storageDir: ".sdd/index/codebase-memory",
  diagnosticsFile: ".sdd/adapters/codebase-memory-mcp/diagnostics.json",
  capabilitiesFile: ".sdd/adapters/codebase-memory-mcp/capabilities.json",
  timeoutMs: 30000,
  fallback: {
    enabled: true,
    provider: "fallback-file-scan",
  },
};

export interface InitResult {
  status: "AVAILABLE" | "DEGRADED" | "FAILED";
  /** MCP 启动结果 */
  lifecycle?: McpLifecycleResult;
  /** 当前 diagnostics */
  diagnostics: McpDiagnostics;
  /** 降级警告（MCP 不可用时） */
  warnings: CliWarning[];
  /** 是否降级 */
  degraded: boolean;
}

/**
 * CodebaseMemoryManager — codebase-memory 包的核心管理器
 *
 * 负责 MCP 生命周期协调、降级处理、diagnostics 写入
 */
export class CodebaseMemoryManager {
  private config: CodebaseConfig;
  private lifecycleResult: McpLifecycleResult | null = null;

  constructor(config: Partial<CodebaseConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  /** 初始化：启动 MCP 或降级 fallback */
  async initialize(root: string): Promise<InitResult> {
    // fallback 模式 — 直接降级
    if (this.config.mode === "fallback") {
      const diag = createDiagnostics({
        actualProvider: "fallback-file-scan",
        mode: "fallback",
        degraded: true,
        indexStatus: "READY",
      });
      await writeDiagnostics(root, this.config.diagnosticsFile, diag);
      return {
        status: "DEGRADED",
        diagnostics: diag,
        warnings: [degradedWarning("配置 mode=fallback")],
        degraded: true,
      };
    }

    // managed 模式 — 启动 npx codebase-memory-mcp
    try {
      const result = await startManagedMcp(
        root,
        this.config.version,
        this.config.timeoutMs,
      );
      this.lifecycleResult = result;

      if (result.status === "STARTED" || result.status === "ALREADY_RUNNING") {
        if (result.session === undefined)
          return this.handleUnavailable(root, "MCP 会话未建立", "initialize");
        await result.session.call("tools/call", {
          name: "index_repository",
          arguments: {
            repo_path: root,
            name: basename(root),
            mode: "fast",
          },
        });
        const diag = createDiagnostics({
          version: this.config.version,
          lastStartedAt: new Date().toISOString(),
          storageDir: this.config.storageDir,
        });
        await writeDiagnostics(root, this.config.diagnosticsFile, diag);
        return {
          status: "AVAILABLE",
          lifecycle: result,
          diagnostics: diag,
          warnings: [],
          degraded: false,
        };
      }

      // MCP 启动返回非成功状态 → 降级
      return this.handleUnavailable(
        root,
        result.message ?? "MCP 启动返回非预期状态",
        "start",
      );
    } catch (err) {
      return this.handleUnavailable(root, (err as Error).message, "start");
    }
  }

  /** 执行结构化查询 */
  async query(input: McpQueryInput): Promise<McpQueryResult> {
    // 如果当前降级中，直接使用 fallback
    if (
      this.config.mode === "fallback" ||
      (this.lifecycleResult &&
        this.lifecycleResult.status !== "STARTED" &&
        this.lifecycleResult.status !== "ALREADY_RUNNING")
    ) {
      return fallbackQuery(input);
    }
    const session = this.lifecycleResult?.session;
    if (session === undefined) return fallbackQuery(input);
    try {
      const name = toolForIntent(input.intent);
      if (!(this.lifecycleResult?.tools ?? []).includes(name))
        throw new Error(`MCP 未暴露 ${name}`);
      const result = await session.call("tools/call", {
        name,
        arguments: toolArguments(name, input),
      });
      return {
        schemaVersion: "1.0.0",
        provider: "codebase-memory-mcp",
        mode: "managed",
        degraded: false,
        intent: input.intent,
        query: input.query,
        items: extractItems(result),
      };
    } catch {
      return fallbackQuery(input);
    }
  }

  /** 获取当前能力描述，基于真实 MCP 状态而非配置 */
  async getCapabilities(): Promise<McpCapabilities> {
    const mcpAlive =
      this.lifecycleResult !== null &&
      (this.lifecycleResult.status === "STARTED" ||
        this.lifecycleResult.status === "ALREADY_RUNNING");
    const degraded = this.config.mode === "fallback" || !mcpAlive;
    const tools = this.lifecycleResult?.tools ?? [];
    const supported = supportedIntents(tools);
    return {
      schemaVersion: "1.0.0",
      provider: degraded ? "fallback-file-scan" : "codebase-memory-mcp",
      supportedIntents: degraded ? ["impact", "related-files"] : supported,
      supportsIndex: !degraded && tools.includes("index_repository"),
      supportsGraphQuery: !degraded && tools.includes("query_graph"),
    };
  }

  /** 停止 MCP */
  stop(): void {
    if (this.lifecycleResult) {
      stopManagedMcp(this.lifecycleResult);
      this.lifecycleResult = null;
    }
  }

  /** 处理 MCP 不可用 → 降级 */
  private async handleUnavailable(
    root: string,
    reason: string,
    stage: string,
  ): Promise<InitResult> {
    // requireAvailable=true 时不得降级
    if (this.config.requireAvailable) {
      const diag = createDiagnostics({
        actualProvider: "codebase-memory-mcp",
        degraded: false,
        indexStatus: "FAILED",
        errors: [createDiagError("E_COMPONENT_UNAVAILABLE", stage, reason)],
      });
      await writeDiagnostics(root, this.config.diagnosticsFile, diag);
      return {
        status: "FAILED",
        diagnostics: diag,
        warnings: [
          {
            code: "E_COMPONENT_UNAVAILABLE",
            message: `codebase-memory-mcp 必须可用但当前不可用: ${reason}`,
            next: "sdd codebase doctor",
          },
        ],
        degraded: false,
      };
    }

    // fallback 启用 → 降级
    if (this.config.fallback.enabled) {
      const diag = createDiagnostics({
        actualProvider: "fallback-file-scan",
        mode: "fallback",
        degraded: true,
        indexStatus: "READY",
        errors: [createDiagError("E_COMPONENT_UNAVAILABLE", stage, reason)],
      });
      await writeDiagnostics(root, this.config.diagnosticsFile, diag);
      return {
        status: "DEGRADED",
        diagnostics: diag,
        warnings: [degradedWarning(reason)],
        degraded: true,
      };
    }

    // fallback 也禁用 → 直接失败
    const diag = createDiagnostics({
      indexStatus: "FAILED",
      errors: [createDiagError("E_COMPONENT_UNAVAILABLE", stage, reason)],
    });
    return {
      status: "FAILED",
      diagnostics: diag,
      warnings: [
        {
          code: "E_COMPONENT_UNAVAILABLE",
          message: `codebase-memory-mcp 不可用且 fallback 已禁用: ${reason}`,
          next: "sdd codebase doctor",
        },
      ],
      degraded: false,
    };
  }
}

function toolForIntent(intent: McpQueryInput["intent"]): string {
  if (intent === "architecture") return "get_architecture";
  if (intent === "callers" || intent === "callees") return "trace_path";
  if (intent === "data-flow") return "trace_path";
  if (intent === "impact") return "detect_changes";
  if (intent === "config" || intent === "related-files") return "search_code";
  return "search_graph";
}

function toolArguments(
  tool: string,
  input: McpQueryInput,
): Record<string, unknown> {
  const project = basename(input.root);
  if (tool === "get_architecture") return { project, aspects: ["overview"] };
  if (tool === "trace_path")
    return {
      project,
      function_name: input.query,
      direction: input.intent === "callers" ? "inbound" : "outbound",
      mode: input.intent === "data-flow" ? "data_flow" : "calls",
    };
  if (tool === "detect_changes") return { project };
  if (tool === "search_code")
    return { project, pattern: input.query, mode: "compact" };
  return { project, query: input.query || ".*" };
}

function supportedIntents(
  tools: string[],
): McpCapabilities["supportedIntents"] {
  const intents: McpCapabilities["supportedIntents"] = [];
  if (tools.includes("detect_changes")) intents.push("impact");
  if (tools.includes("search_code")) intents.push("related-files", "config");
  if (tools.includes("search_graph"))
    intents.push("symbols", "routes", "tests", "entrypoints");
  if (tools.includes("trace_path"))
    intents.push("callers", "callees", "data-flow");
  if (tools.includes("get_architecture")) intents.push("architecture");
  return intents;
}

function extractItems(value: unknown): McpQueryResult["items"] {
  const text = Array.isArray((value as { content?: unknown })?.content)
    ? (value as { content: Array<{ text?: unknown }> }).content
        .map((item) => item.text)
        .filter((item): item is string => typeof item === "string")
        .join("\n")
    : "";
  let payload: unknown = value;
  try {
    payload = text.length === 0 ? value : JSON.parse(text);
  } catch {
    return [{ type: "module", confidence: 0.5, reason: text.slice(0, 500) }];
  }
  const record = payload as Record<string, unknown>;
  const results = Array.isArray(record.results)
    ? record.results
    : Array.isArray(record.items)
      ? record.items
      : [];
  return results.flatMap((entry) => {
    if (typeof entry !== "object" || entry === null) return [];
    const item = entry as Record<string, unknown>;
    const path =
      typeof item.file_path === "string" ? item.file_path : item.path;
    const symbol =
      typeof item.qualified_name === "string"
        ? item.qualified_name
        : typeof item.name === "string"
          ? item.name
          : undefined;
    return [
      {
        type: typeof symbol === "string" ? "symbol" : "file",
        ...(typeof path === "string" ? { path } : {}),
        ...(typeof symbol === "string" ? { symbol } : {}),
        confidence: 0.9,
        reason: "codebase-memory-mcp 查询结果",
      },
    ];
  });
}
