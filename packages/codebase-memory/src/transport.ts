import type {
  CodebaseSummary,
  McpDiagnostics as AdapterMcpDiagnostics,
  McpTransport,
  McpQueryInput,
} from "@sdd-harness/core";
import { CodebaseMemoryManager } from "./manager.js";
import type { InitResult } from "./manager.js";

/**
 * CodebaseMemoryTransport — 将 CodebaseMemoryManager 适配为 Core 期望的 McpTransport 接口
 *
 * isAvailable() 返回 true 告知 adapter "可以尝试启动 MCP"，
 * index(root) 实际调用 manager.initialize(root) 启动 npx 进程。
 * adapter 内部有 try/catch，启动失败时自动走 fallback。
 */
export class CodebaseMemoryTransport implements McpTransport {
  private manager: CodebaseMemoryManager;
  private initialization: InitResult | null = null;

  constructor(manager?: CodebaseMemoryManager) {
    this.manager = manager ?? new CodebaseMemoryManager();
  }

  /** 只有 initialize 已完成且未降级时才报告 MCP 可用。 */
  async isAvailable(): Promise<boolean> {
    return (
      this.initialization === null || this.initialization.degraded === false
    );
  }

  async index(
    root: string,
  ): Promise<{ degraded: boolean; failed?: boolean; reason?: string }> {
    this.initialization = await this.manager.initialize(root);
    const reason = this.initialization.diagnostics.errors.at(-1)?.message;
    const result = {
      degraded: this.initialization.degraded,
      ...(this.initialization.status === "FAILED" ? { failed: true } : {}),
    };
    return reason === undefined ? result : { ...result, reason };
  }

  async summarize(root: string): Promise<CodebaseSummary> {
    const caps = await this.manager.getCapabilities();
    const degraded = caps.provider === "fallback-file-scan";
    return {
      codebaseSummary: degraded
        ? "codebase-memory-mcp 不可用，使用 fallback-file-scan"
        : `codebase-memory-mcp 已索引：${root}`,
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
        payload: { files: [], symbols: [], tests: [], risks: [] },
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
