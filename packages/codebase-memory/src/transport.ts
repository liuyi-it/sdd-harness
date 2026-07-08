import type {
  CodebaseSummary,
  McpDiagnostics as AdapterMcpDiagnostics,
  McpTransport,
  McpQueryInput,
} from "@sdd-harness/core";
import type { CodebaseMemoryManager } from "./manager.js";

/**
 * CodebaseMemoryTransport — 将 CodebaseMemoryManager 适配为 Core 期望的 McpTransport 接口
 *
 * 这是 codebase-memory 包和 Core 之间的桥梁：
 * - Core 通过 McpTransport 接口使用 codebase 能力
 * - CodebaseMemoryManager 负责 npx 托管、降级、diagnostics
 * - 本类负责接口适配和协议转换
 */
export class CodebaseMemoryTransport implements McpTransport {
  private initialized = false;

  constructor(private readonly manager: CodebaseMemoryManager) {}

  async isAvailable(): Promise<boolean> {
    if (!this.initialized) return false;
    const caps = await this.manager.getCapabilities();
    return caps.provider === "codebase-memory-mcp";
  }

  async index(root: string): Promise<void> {
    this.initialized = true;
    await this.manager.initialize(root);
  }

  async summarize(root: string): Promise<CodebaseSummary> {
    void root;
    const caps = await this.manager.getCapabilities();
    const degraded = caps.provider === "fallback-file-scan";
    return {
      codebaseSummary: degraded
        ? "codebase-memory-mcp 不可用，使用 fallback-file-scan"
        : "codebase-memory-mcp 托管运行中",
      packageStructure: "",
      architecture: degraded ? "fallback" : "mcp-managed",
    };
  }

  async inspect(root: string): Promise<Partial<AdapterMcpDiagnostics>> {
    void root;
    const caps = await this.manager.getCapabilities();
    const degraded = caps.provider === "fallback-file-scan";
    return {
      installed: !degraded,
      configured: !degraded,
      connected: !degraded,
      callable: !degraded,
      indexed: !degraded,
      officialUrl: "https://github.com/liuyi-it/codebase-memory-mcp",
    };
  }

  async capabilities(root: string): Promise<string[]> {
    void root;
    const caps = await this.manager.getCapabilities();
    return caps.supportedIntents;
  }

  async query(root: string, input: McpQueryInput): Promise<unknown> {
    const result = await this.manager.query({
      intent: input.intent,
      query: input.requirement ?? input.intent,
      root,
    });
    if (result.degraded) {
      return {
        provider: result.provider,
        intent: result.intent,
        degraded: true,
        reason: "MCP unavailable, using fallback",
        confidence: 0.3,
        payload: {
          files: [],
          symbols: [],
          tests: [],
          risks: [],
        },
      };
    }
    return {
      provider: result.provider,
      intent: result.intent,
      degraded: false,
      payload: {
        files: result.items
          .filter((i) => i.type === "file")
          .map((i) => i.path ?? ""),
        symbols: result.items
          .filter((i) => i.type === "symbol")
          .map((i) => i.symbol ?? ""),
        tests: result.items
          .filter((i) => i.type === "test")
          .map((i) => i.path ?? ""),
        risks: [],
      },
    };
  }
}
