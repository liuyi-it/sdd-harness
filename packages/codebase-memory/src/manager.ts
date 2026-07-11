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
  // 请求超时而非进程生命周期超时；默认值保证 CLI 在 MCP 不可达时快速降级。
  timeoutMs: 5000,
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
        await this.callWithTimeout(
          result.session.call("tools/call", {
            name: "index_repository",
            arguments: {
              repo_path: root,
              name: basename(root),
              mode: "fast",
            },
          }),
          "index_repository",
        );
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
    if (session === undefined || !session.isAlive()) {
      if (this.lifecycleResult !== null) this.lifecycleResult.status = "FAILED";
      return fallbackQuery(input);
    }
    try {
      const name = toolForIntent(input.intent);
      if (!(this.lifecycleResult?.tools ?? []).includes(name))
        throw new Error(`MCP 未暴露 ${name}`);
      const responses = await collectPaginatedToolResults(
        (argumentsValue) =>
          this.callWithTimeout(
            session.call("tools/call", { name, arguments: argumentsValue }),
            name,
          ),
        toolArguments(name, input),
      );
      return {
        schemaVersion: "1.0.0",
        provider: "codebase-memory-mcp",
        mode: "managed",
        degraded: false,
        intent: input.intent,
        query: input.query,
        items: dedupeItems(
          responses.flatMap((response) =>
            decodeToolResult(input.intent, response),
          ),
        ),
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
        this.lifecycleResult.status === "ALREADY_RUNNING") &&
      this.lifecycleResult.session?.isAlive() === true;
    const degraded = this.config.mode === "fallback" || !mcpAlive;
    const tools = this.lifecycleResult?.tools ?? [];
    const supported = supportedIntents(tools);
    return {
      schemaVersion: "1.0.0",
      provider: degraded ? "fallback-file-scan" : "codebase-memory-mcp",
      availableTools: degraded ? [] : [...tools],
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

  async isAvailable(): Promise<boolean> {
    return (
      this.lifecycleResult !== null &&
      (this.lifecycleResult.status === "STARTED" ||
        this.lifecycleResult.status === "ALREADY_RUNNING") &&
      this.lifecycleResult.session?.isAlive() === true
    );
  }

  private async callWithTimeout<T>(
    promise: Promise<T>,
    operation: string,
  ): Promise<T> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      return await Promise.race([
        promise,
        new Promise<T>((_, reject) => {
          timer = setTimeout(
            () =>
              reject(
                new Error(`MCP ${operation} 超时 (${this.config.timeoutMs}ms)`),
              ),
            this.config.timeoutMs,
          );
        }),
      ]);
    } finally {
      if (timer !== undefined) clearTimeout(timer);
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

export function decodeToolResult(
  intent: McpQueryInput["intent"],
  value: unknown,
): McpQueryResult["items"] {
  const envelope = value as {
    isError?: unknown;
    structuredContent?: unknown;
    content?: unknown;
  };
  if (envelope?.isError === true) throw new Error("MCP tools/call 返回错误");
  const payload = unwrapToolPayload(value);
  const records = collectRecords(payload);
  const items = records.flatMap((record) => decodeRecord(intent, record));
  if (items.length === 0 && records.length > 0)
    throw new Error(`无法解码 ${intent} MCP 响应`);
  return dedupeItems(items);
}

function unwrapToolPayload(value: unknown): unknown {
  const envelope = value as { structuredContent?: unknown; content?: unknown };
  if (envelope?.structuredContent !== undefined)
    return envelope.structuredContent;
  if (!Array.isArray(envelope?.content)) return value;
  const text = (envelope.content as Array<{ text?: unknown }>)
    .map((item) => item.text)
    .filter((item): item is string => typeof item === "string")
    .join("\n");
  if (text.length === 0) return value;
  try {
    return JSON.parse(text);
  } catch {
    throw new Error("MCP tools/call 文本响应不是有效 JSON");
  }
}

export async function collectPaginatedToolResults(
  callPage: (argumentsValue: Record<string, unknown>) => Promise<unknown>,
  initialArguments: Record<string, unknown>,
): Promise<unknown[]> {
  const responses: unknown[] = [];
  let argumentsValue = initialArguments;
  for (let pageIndex = 0; pageIndex < 100; pageIndex += 1) {
    const response = await callPage(argumentsValue);
    responses.push(response);
    const page = pagination(response);
    if (!page.hasMore) break;
    argumentsValue = {
      ...argumentsValue,
      offset:
        (typeof argumentsValue.offset === "number"
          ? argumentsValue.offset
          : 0) + Math.max(page.count, 1),
    };
  }
  return responses;
}

function pagination(value: unknown): { hasMore: boolean; count: number } {
  const payload = unwrapToolPayload(value) as Record<string, unknown>;
  if (typeof payload !== "object" || payload === null)
    return { hasMore: false, count: 0 };
  const results = Array.isArray(payload.results)
    ? payload.results
    : Array.isArray(payload.items)
      ? payload.items
      : [];
  return { hasMore: payload.has_more === true, count: results.length };
}

function collectRecords(value: unknown): Array<Record<string, unknown>> {
  if (Array.isArray(value)) return value.flatMap(collectRecords);
  if (typeof value !== "object" || value === null) return [];
  const record = value as Record<string, unknown>;
  const containers: Array<[string, unknown]> = [
    ["results", record.results],
    ["items", record.items],
    ["files", record.files],
    ["changed_files", record.changed_files],
    ["affected_files", record.affected_files],
    ["symbols", record.symbols],
    ["affected_symbols", record.affected_symbols],
    ["routes", record.routes],
    ["tests", record.tests],
    ["nodes", record.nodes],
    ["paths", record.paths],
    ["packages", record.packages],
    ["entry_points", record.entry_points],
    ["risks", record.risks],
  ];
  const hasRecognizedContainer = containers.some(
    ([, child]) => child !== undefined,
  );
  const nested = containers.flatMap(([key, child]) => {
    if (!Array.isArray(child)) return collectRecords(child);
    return child.flatMap((entry) => {
      if (typeof entry !== "string") return collectRecords(entry);
      if (key.includes("symbol") || key === "entry_points" || key === "risks")
        return [{ symbol: entry, type: key }];
      return [{ path: entry, type: key }];
    });
  });
  // MCP 的合法空查询通常是 `{ results: [] }`。它不是未知响应，也不能
  // 触发 fallback；只有完全没有可识别容器时才把对象本身交给 decoder。
  return hasRecognizedContainer ? nested : [record];
}

function decodeRecord(
  intent: McpQueryInput["intent"],
  record: Record<string, unknown>,
): McpQueryResult["items"] {
  const path = firstString(record, ["file_path", "path", "file", "source"]);
  const symbol = firstString(record, [
    "qualified_name",
    "symbol",
    "function_name",
    "name",
  ]);
  const label = firstString(record, ["label", "type", "kind"]);
  if (path === undefined && symbol === undefined) return [];
  const inferredType = /risk/i.test(label ?? "")
    ? "risk"
    : intent === "tests" || /test/i.test(label ?? "")
      ? "test"
      : intent === "routes" || /route/i.test(label ?? "")
        ? "route"
        : intent === "architecture"
          ? "module"
          : symbol !== undefined
            ? "symbol"
            : "file";
  return [
    {
      type: inferredType,
      ...(path === undefined ? {} : { path }),
      ...(symbol === undefined ? {} : { symbol }),
      confidence: 0.9,
      reason: `codebase-memory-mcp ${intent} 查询结果`,
    },
  ];
}

function firstString(
  record: Record<string, unknown>,
  keys: string[],
): string | undefined {
  for (const key of keys)
    if (typeof record[key] === "string" && record[key].length > 0)
      return record[key];
  return undefined;
}

function dedupeItems(items: McpQueryResult["items"]): McpQueryResult["items"] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const key = `${item.type}\0${item.path ?? ""}\0${item.symbol ?? ""}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
