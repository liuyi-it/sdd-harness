import type { CliWarning } from "@sdd-harness/core";
import { startManagedMcp, stopManagedMcp } from "./lifecycle.js";
import { createDiagnostics, createDiagError, writeDiagnostics } from "./diagnostics.js";
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

const DEFAULT_CONFIG: CodebaseConfig = {
  provider: "codebase-memory-mcp",
  mode: "managed",
  version: "0.8.1",
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
        const diag = createDiagnostics({
          version: this.config.version,
          lastStartedAt: new Date().toISOString(),
          storageDir: this.config.storageDir,
        });
        await writeDiagnostics(root, this.config.diagnosticsFile, diag);
        return {
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
      return this.handleUnavailable(
        root,
        (err as Error).message,
        "start",
      );
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
    // 正常模式 — 此处后续可通过 MCP stdio 发送 query 请求
    // 当前 MVP 返回 fallback 结果
    return fallbackQuery(input);
  }

  /** 获取当前能力描述 */
  async getCapabilities(): Promise<McpCapabilities> {
    return {
      schemaVersion: "1.0.0",
      provider: this.config.mode === "fallback"
        ? "fallback-file-scan"
        : "codebase-memory-mcp",
      supportedIntents: ["impact", "related-files", "symbols", "tests"],
      supportsIndex: this.config.mode !== "fallback",
      supportsGraphQuery: this.config.mode !== "fallback",
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
