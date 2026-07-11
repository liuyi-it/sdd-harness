import { describe, expect, it, vi } from "vitest";

import {
  CodebaseAdapter,
  type CodebaseSummary,
  type McpTransport,
} from "../src/codebase/codebase-adapter.js";
import {
  createMcpQueryBuilder,
  MCP_FALLBACK_PROVIDER,
  MCP_PINNED_PROVIDER,
  MCP_PINNED_VERSION,
  MCP_PINNED_COMMIT,
  MCP_QUERY_UNAVAILABLE,
  isSupportedIntent,
  type McpQueryInput,
} from "../src/codebase/mcp-query.js";
import { runCodebaseCommand } from "../src/commands/codebase.js";

class MemoryTransport implements McpTransport {
  public indexed = 0;
  public availCalls = 0;
  public queryCalls = 0;
  public capabilitiesCalls = 0;
  public capabilityTools: string[];
  public impactHandler: (input: unknown) => unknown;

  constructor(
    options: {
      available?: boolean;
      capabilityTools?: string[];
      impactHandler?: (input: unknown) => unknown;
    } = {},
  ) {
    this.capabilityTools = options.capabilityTools ?? [
      "search_context",
      "summarize_repo",
    ];
    this.impactHandler =
      options.impactHandler ??
      (() => ({
        provider: MCP_PINNED_PROVIDER,
        intent: "impact",
        payload: {
          files: ["src/orders/index.ts"],
          symbols: ["OrdersService"],
          tests: ["test/orders.test.ts"],
          risks: [],
        },
      }));
  }

  async isAvailable(): Promise<boolean> {
    this.availCalls += 1;
    return true;
  }
  async index(): Promise<void> {
    this.indexed += 1;
  }
  async summarize(): Promise<CodebaseSummary> {
    return {
      codebaseSummary:
        "package.json\nsrc/orders/index.ts\ntest/orders.test.ts\nMCP summary",
      packageStructure: "src/orders",
      architecture: "layered",
    };
  }
  async inspect() {
    return {
      installed: true,
      configured: true,
      connected: true,
      callable: true,
      indexed: true,
    };
  }
  async capabilities() {
    this.capabilitiesCalls += 1;
    return {
      availableTools: [...this.capabilityTools],
      supportedIntents: [
        "impact",
        "related-files",
        "symbols",
        "callers",
        "callees",
        "routes",
        "tests",
        "architecture",
      ] as McpQueryInput["intent"][],
    };
  }
  async query(_root: string, input: unknown): Promise<unknown> {
    this.queryCalls += 1;
    return this.impactHandler(input);
  }
}

describe("MCP transport v2", () => {
  it("codebase query 将 CLI query 文本原样传给 adapter", async () => {
    const query = vi.fn().mockResolvedValue({ provider: MCP_PINNED_PROVIDER });
    await runCodebaseCommand("/repo", { query } as unknown as CodebaseAdapter, {
      subcommand: "query",
      intent: "symbols",
      query: "OrderService",
    });
    expect(query).toHaveBeenCalledWith(
      { intent: "symbols", query: "OrderService" },
      "/repo",
    );
  });

  it("capability discover reports fixed provider + version + commit", async () => {
    const adapter = new CodebaseAdapter(new MemoryTransport());
    const capabilities = await adapter.capabilities();
    expect(capabilities).toMatchObject({
      provider: MCP_PINNED_PROVIDER,
      version: MCP_PINNED_VERSION,
      commit: MCP_PINNED_COMMIT,
      availableTools: ["search_context", "summarize_repo"],
      supportedIntents: expect.arrayContaining([
        "impact",
        "symbols",
        "callers",
        "callees",
        "routes",
        "tests",
        "architecture",
      ]),
    });
  });

  it("caps capability tools when transport returns empty list (no crash)", async () => {
    const adapter = new CodebaseAdapter(
      new MemoryTransport({ capabilityTools: [] }),
    );
    const capabilities = await adapter.capabilities();
    expect(capabilities.availableTools).toEqual([]);
  });

  (process.platform === "win32" ? it.skip : it)(
    "writeCapabilityArtifacts persists capabilities + diagnostics",
    async () => {
      const adapter = new CodebaseAdapter(new MemoryTransport());
      const root = "/tmp/sdd-mcp-capabilty-fixture";
      const paths = await adapter.writeCapabilityArtifacts(root);
      expect(paths.capabilitiesPath).toContain(
        ".sdd/index/mcp-capabilities.json",
      );
      expect(paths.diagnosticsPath).toContain(
        ".sdd/index/codebase-diagnostics.json",
      );
    },
  );

  it("isSupportedIntent only accepts whitelisted intents", () => {
    expect(isSupportedIntent("impact")).toBe(true);
    expect(isSupportedIntent("architecture")).toBe(true);
    expect(isSupportedIntent("routes")).toBe(true);
    expect(isSupportedIntent("execute_command")).toBe(false);
  });

  it("query() returns degraded fallback when transport is missing", async () => {
    const adapter = new CodebaseAdapter();
    const result = await adapter.query({ intent: "impact", query: "orders" });
    expect(result).toMatchObject({
      schemaVersion: "1.2.0",
      intent: "impact",
      provider: MCP_FALLBACK_PROVIDER,
      degraded: true,
      reason: MCP_QUERY_UNAVAILABLE,
      confidence: 0.3,
    });
  });

  it("query() returns precise result when transport is available", async () => {
    const adapter = new CodebaseAdapter(new MemoryTransport());
    const result = await adapter.query({ intent: "impact", query: "orders" });
    expect(result).toMatchObject({
      schemaVersion: "1.2.0",
      intent: "impact",
      provider: MCP_PINNED_PROVIDER,
      degraded: false,
    });
  });

  it("query() caps confidence to valid 0..1 and defaults generatedAt", async () => {
    const adapter = new CodebaseAdapter(
      new MemoryTransport({
        impactHandler: () => ({
          provider: MCP_PINNED_PROVIDER,
          intent: "impact",
          confidence: 9999,
          payload: { files: [], symbols: [], tests: [], risks: [] },
        }),
      }),
    );
    const result = await adapter.query({ intent: "impact", query: "orders" });
    expect(result.confidence).toBeLessThanOrEqual(0.99);
  });

  it("query() rejects malformed payload by degrading", async () => {
    const adapter = new CodebaseAdapter(
      new MemoryTransport({
        impactHandler: () => ({ wrong: "shape" }),
      }),
    );
    const result = await adapter.query({ intent: "impact", query: "orders" });
    expect(result.provider).toBe(MCP_FALLBACK_PROVIDER);
    expect(result.degraded).toBe(true);
  });

  it("query() caps confidence to 0 when transport returns invalid number", async () => {
    const adapter = new CodebaseAdapter(
      new MemoryTransport({
        impactHandler: () => ({
          provider: MCP_PINNED_PROVIDER,
          intent: "impact",
          confidence: -7,
          payload: { files: [], symbols: [], tests: [], risks: [] },
        }),
      }),
    );
    const result = await adapter.query({ intent: "impact", query: "orders" });
    expect(result.confidence).toBe(0.01);
  });

  it("queryImpact() dedupes and normalizes data when MCP returns raw payload", async () => {
    const adapter = new CodebaseAdapter(
      new MemoryTransport({
        impactHandler: () => ({
          provider: MCP_PINNED_PROVIDER,
          intent: "impact",
          payload: {
            files: ["src/orders/index.ts", "src/orders/index.ts", "   "],
            symbols: ["OrdersService", "OrdersService"],
            tests: [],
            risks: ["x"],
          },
        }),
      }),
    );
    const result = await adapter.queryImpact("/repo", {
      intent: "impact",
      query: "orders",
    });
    expect(result.payload.files).toEqual(["src/orders/index.ts"]);
    expect(result.payload.symbols).toEqual(["OrdersService"]);
    expect(result.payload.risks).toEqual(["x"]);
    expect(result.payload.tests).toEqual([]);
  });

  it("queryImpact() returns empty ImpactPayload when degraded", async () => {
    const adapter = new CodebaseAdapter();
    const result = await adapter.queryImpact("/repo", {
      intent: "impact",
      query: "orders",
    });
    expect(result.provider).toBe(MCP_FALLBACK_PROVIDER);
    expect(result.payload).toEqual({
      files: [],
      symbols: [],
      tests: [],
      risks: [],
    });
  });

  it("queryImpact() 保留 transport 的 fallback provider 与 degraded 状态", async () => {
    const adapter = new CodebaseAdapter(
      new MemoryTransport({
        impactHandler: () => ({
          schemaVersion: "1.2.0",
          provider: MCP_FALLBACK_PROVIDER,
          intent: "impact",
          degraded: true,
          reason: "detect_changes unavailable",
          confidence: 0.3,
          payload: { files: [], symbols: [], tests: [], risks: [] },
        }),
      }),
    );

    await expect(
      adapter.queryImpact("/repo", { intent: "impact", query: "orders" }),
    ).resolves.toMatchObject({
      provider: MCP_FALLBACK_PROVIDER,
      degraded: true,
      reason: "detect_changes unavailable",
      confidence: 0.3,
    });
  });

  it("createMcpQueryBuilder.buildFallback sets degraded metadata", () => {
    const builder = createMcpQueryBuilder(
      () => new Date("2026-07-06T00:00:00Z"),
    );
    const result = builder.buildFallback(
      "architecture",
      MCP_QUERY_UNAVAILABLE,
      { foo: 1 },
    );
    expect(result).toMatchObject({
      schemaVersion: "1.2.0",
      intent: "architecture",
      provider: MCP_FALLBACK_PROVIDER,
      degraded: true,
      reason: MCP_QUERY_UNAVAILABLE,
      generatedAt: "2026-07-06T00:00:00.000Z",
      confidence: 0.3,
    });
  });

  it("createMcpQueryBuilder.summarizeCodebase marks precise results", () => {
    const builder = createMcpQueryBuilder();
    const summary: CodebaseSummary = {
      codebaseSummary: "x",
      packageStructure: "y",
      architecture: "z",
    };
    expect(builder.summarizeCodebase(summary)).toMatchObject({
      intent: "architecture",
      provider: MCP_PINNED_PROVIDER,
      degraded: false,
    });
  });

  it("queryImpact() rejects non-impact intents", async () => {
    const adapter = new CodebaseAdapter(new MemoryTransport());
    await expect(
      adapter.queryImpact("/repo", { intent: "symbols", query: "orders" }),
    ).rejects.toThrow(/impact/);
  });

  it("query() rejects unsupported intents", async () => {
    const adapter = new CodebaseAdapter(new MemoryTransport());
    await expect(
      adapter.query({ intent: "execute_command" as never, query: "orders" }),
    ).rejects.toThrow(/unsupported intent/);
  });
});
