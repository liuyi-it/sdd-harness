import { mkdir, opendir, readFile, writeFile } from "node:fs/promises";
import { join, relative, resolve } from "node:path";

import {
  createMcpQueryBuilder,
  isSupportedIntent,
  MCP_FALLBACK_PROVIDER,
  MCP_PINNED_PROVIDER,
  MCP_QUERY_UNAVAILABLE,
  type ImpactPayload,
  type McpCapabilities,
  type McpQueryInput,
  type McpQueryResult,
} from "./mcp-query.js";

/**
 * CodebaseAdapter 封装两类代码上下文来源：
 * 1. 优先使用 codebase-memory-mcp (pinned v0.9.0 / b637e33)
 * 2. 不可用时退回受限文件扫描
 *
 * 二期新增 V2 capability + 结构化 query 接口，所有跨 provider 一致性由本类负责。
 */
export interface CodebaseSummary {
  codebaseSummary: string;
  packageStructure: string;
  architecture: string;
}

export interface McpDiagnostics {
  installed: boolean;
  configured: boolean;
  connected: boolean;
  callable: boolean;
  indexed: boolean;
  officialUrl: string;
  message?: string;
}

export interface CodebaseResult extends CodebaseSummary {
  provider: "codebase-memory-mcp" | "fallback-file-scan";
  degraded: boolean;
  reason?: string;
  diagnostics: McpDiagnostics;
}

export const MCP_UNAVAILABLE_REASON = "codebase-memory-mcp unavailable";

export interface McpTransport {
  isAvailable(): Promise<boolean>;
  index(root: string): Promise<void>;
  summarize(root: string): Promise<CodebaseSummary>;
  inspect?(root: string): Promise<Partial<McpDiagnostics>>;
  /** V2: 返回 MCP 暴露的工具集合，缺失时按空集合处理并写入 partial。 */
  capabilities?(root: string): Promise<string[]>;
  /**
   * V2: 结构化查询；缺失时 Core 自动以 fallback-file-scan 返回
   * degraded=true 的同结构结果，禁止调用方自定义 payload shape。
   */
  query?(root: string, input: McpQueryInput): Promise<unknown>;
}

const EXCLUDED_DIRECTORIES = new Set([
  ".git",
  ".sdd",
  "node_modules",
  "target",
  "build",
  "dist",
  "coverage",
  "logs",
]);

const SUMMARY_FILE_LIMIT = 200;
const CANDIDATE_FILE_LIMIT = 6;
const KEYWORD_FILE_LIMIT = 80;
const SAFE_TEXT_EXTENSIONS = [
  ".ts",
  ".tsx",
  ".js",
  ".jsx",
  ".json",
  ".md",
  ".yml",
  ".yaml",
  ".java",
] as const;

const KEYWORD_RULES = [
  { label: "service", patterns: ["service", "Service"] },
  { label: "controller", patterns: ["controller", "Controller"] },
  { label: "route", patterns: ["route", "router", "endpoint"] },
  { label: "test", patterns: ["describe(", "it(", "test("] },
] as const;

export class CodebaseAdapter {
  constructor(private readonly transport?: McpTransport) {}

  async initialize(root: string): Promise<CodebaseResult> {
    const inspected = await this.transport?.inspect?.(root);
    if (this.transport !== undefined) {
      try {
        if (await this.transport.isAvailable()) {
          await this.transport.index(root);
          return {
            provider: "codebase-memory-mcp",
            degraded: false,
            ...(await this.transport.summarize(root)),
            diagnostics: {
              installed: true,
              configured: true,
              connected: true,
              callable: true,
              indexed: true,
              officialUrl: CODEBASE_MEMORY_MCP_URL,
              ...inspected,
            },
          };
        }
      } catch (error) {
        return await this.fallback(root, {
          installed: inspected?.installed ?? true,
          configured: inspected?.configured ?? true,
          connected: inspected?.connected ?? false,
          callable: false,
          indexed: false,
          officialUrl: CODEBASE_MEMORY_MCP_URL,
          message: error instanceof Error ? error.message : String(error),
        });
      }
      return await this.fallback(root, {
        installed: inspected?.installed ?? false,
        configured: inspected?.configured ?? false,
        connected: inspected?.connected ?? false,
        callable: inspected?.callable ?? false,
        indexed: inspected?.indexed ?? false,
        officialUrl: CODEBASE_MEMORY_MCP_URL,
        ...(inspected?.message === undefined
          ? {}
          : { message: inspected.message }),
      });
    }
    return await this.fallback(root, {
      installed: false,
      configured: false,
      connected: false,
      callable: false,
      indexed: false,
      officialUrl: CODEBASE_MEMORY_MCP_URL,
    });
  }

  /**
   * V2 capability discovery：返回 MCP 固定版本的工具清单；缺失时按 partial 写入，
   * 仍必须保留 officialUrl、version、commit 三项事实。
   */
  async capabilities(): Promise<McpCapabilities> {
    const builder = createMcpQueryBuilder();
    if (
      this.transport === undefined ||
      (await this.transport.isAvailable()) === false
    ) {
      return {
        provider: MCP_PINNED_PROVIDER,
        version: "0.0.0",
        commit: "b637e33",
        officialUrl: CODEBASE_MEMORY_MCP_URL,
        availableTools: [],
        supportedIntents: [
          "impact",
          "related-files",
          "symbols",
          "callers",
          "callees",
          "routes",
          "tests",
          "architecture",
        ],
        generatedAt: new Date().toISOString(),
      };
    }
    const tools =
      (await this.transport.capabilities?.(".").catch(() => [])) ?? [];
    return builder.capabilitiesFrom(tools);
  }

  /**
   * V2 query：唯一允许通过 Core 访问代码库上下文的入口。任何 transport.query
   * 返回必须命中 ImpactPayload / CodebaseSummary 等已知结构；其余 shape 一律降级。
   */
  async query<TPayload = unknown>(
    input: McpQueryInput,
  ): Promise<McpQueryResult<TPayload>> {
    if (!isSupportedIntent(input.intent)) {
      throw new Error(`unsupported intent: ${input.intent}`);
    }
    const builder = createMcpQueryBuilder();
    if (this.transport === undefined) {
      return builder.buildFallback<TPayload>(
        input.intent,
        MCP_QUERY_UNAVAILABLE,
        { intent: input.intent } as unknown as TPayload,
      );
    }
    const available = await this.transport.isAvailable().catch(() => false);
    if (available === false || this.transport.query === undefined) {
      return builder.buildFallback<TPayload>(
        input.intent,
        MCP_QUERY_UNAVAILABLE,
        { intent: input.intent } as unknown as TPayload,
      );
    }
    try {
      const raw = await this.transport.query(".", input);
      if (raw === null || typeof raw !== "object") {
        return builder.buildFallback<TPayload>(
          input.intent,
          MCP_QUERY_UNAVAILABLE,
          { intent: input.intent } as unknown as TPayload,
        );
      }
      const candidate = raw as McpQueryResult<TPayload>;
      if (
        typeof candidate.provider !== "string" ||
        (candidate.provider !== MCP_PINNED_PROVIDER &&
          candidate.provider !== MCP_FALLBACK_PROVIDER)
      ) {
        return builder.buildFallback<TPayload>(
          input.intent,
          MCP_QUERY_UNAVAILABLE,
          { intent: input.intent } as unknown as TPayload,
        );
      }
      return {
        ...candidate,
        schemaVersion: "1.2.0",
        intent: input.intent,
        provider: candidate.provider,
        degraded: candidate.degraded === true,
        confidence: clampConfidence(candidate.confidence),
        generatedAt: candidate.generatedAt ?? new Date().toISOString(),
        payload: candidate.payload,
      };
    } catch (error) {
      return builder.buildFallback<TPayload>(
        input.intent,
        `MCP query failed: ${error instanceof Error ? error.message : String(error)}`,
        { intent: input.intent } as unknown as TPayload,
      );
    }
  }

  async writeCapabilityArtifacts(
    root: string,
  ): Promise<{ capabilitiesPath: string; diagnosticsPath: string }> {
    const sddRoot = join(root, ".sdd");
    await mkdir(join(sddRoot, "index"), { recursive: true });
    const capabilities = await this.capabilities();
    const diagnostics = await this.inspectDiagnostics(root);
    const capabilitiesPath = join(sddRoot, "index", "mcp-capabilities.json");
    const diagnosticsPath = join(sddRoot, "index", "codebase-diagnostics.json");
    await writeFile(
      capabilitiesPath,
      `${JSON.stringify(capabilities, null, 2)}
`,
      "utf8",
    );
    await writeFile(
      diagnosticsPath,
      `${JSON.stringify(diagnostics, null, 2)}
`,
      "utf8",
    );
    return { capabilitiesPath, diagnosticsPath };
  }

  async queryImpact(
    root: string,
    input: McpQueryInput,
  ): Promise<McpQueryResult<ImpactPayload>> {
    if (input.intent !== "impact") {
      throw new Error("queryImpact only supports intent=impact");
    }
    const builder = createMcpQueryBuilder();
    if (this.transport === undefined) {
      return builder.buildFallback<ImpactPayload>(
        "impact",
        MCP_QUERY_UNAVAILABLE,
        {
          files: [],
          symbols: [],
          tests: [],
          risks: [],
        },
      );
    }
    const available = await this.transport.isAvailable().catch(() => false);
    if (available === false || this.transport.query === undefined) {
      return builder.buildFallback<ImpactPayload>(
        "impact",
        MCP_QUERY_UNAVAILABLE,
        {
          files: [],
          symbols: [],
          tests: [],
          risks: [],
        },
      );
    }
    try {
      const raw = await this.transport.query(root, input);
      if (raw === null || typeof raw !== "object") {
        return builder.buildFallback<ImpactPayload>(
          "impact",
          MCP_QUERY_UNAVAILABLE,
          {
            files: [],
            symbols: [],
            tests: [],
            risks: [],
          },
        );
      }
      const payload = coerceImpactPayload(
        (raw as { payload?: unknown }).payload,
      );
      return builder.buildImpactResult(input, payload);
    } catch (error) {
      return builder.buildFallback<ImpactPayload>(
        "impact",
        `MCP impact query failed: ${error instanceof Error ? error.message : String(error)}`,
        { files: [], symbols: [], tests: [], risks: [] },
      );
    }
  }

  private async inspectDiagnostics(root: string): Promise<McpDiagnostics> {
    const fallback: McpDiagnostics = {
      installed: this.transport !== undefined,
      configured: this.transport !== undefined,
      connected: false,
      callable: false,
      indexed: false,
      officialUrl: CODEBASE_MEMORY_MCP_URL,
    };
    if (this.transport === undefined) return fallback;
    const inspected = await this.transport
      .inspect?.(root)
      .catch(() => undefined);
    if (inspected === undefined) return fallback;
    return {
      installed:
        typeof inspected.installed === "boolean"
          ? inspected.installed
          : fallback.installed,
      configured:
        typeof inspected.configured === "boolean"
          ? inspected.configured
          : fallback.configured,
      connected:
        typeof inspected.connected === "boolean" ? inspected.connected : false,
      callable:
        typeof inspected.callable === "boolean" ? inspected.callable : false,
      indexed:
        typeof inspected.indexed === "boolean" ? inspected.indexed : false,
      officialUrl:
        typeof inspected.officialUrl === "string"
          ? inspected.officialUrl
          : fallback.officialUrl,
      ...(inspected.message === undefined
        ? {}
        : { message: inspected.message }),
    };
  }

  private async fallback(
    root: string,
    diagnostics: McpDiagnostics,
  ): Promise<CodebaseResult> {
    const files = await scanFiles(root);
    const directories = [
      ...new Set(files.map((file) => file.split("/").slice(0, -1).join("/"))),
    ]
      .filter(Boolean)
      .sort();
    const keywordMatches = await collectKeywordMatches(root, files);
    const candidateSummaries = await readCandidateSummaries(root, files);
    return {
      provider: "fallback-file-scan",
      degraded: true,
      reason: MCP_QUERY_UNAVAILABLE,
      diagnostics,
      codebaseSummary: [
        "# 代码库摘要",
        "",
        "当前使用 fallback-file-scan 降级模式，以下结果仅来自受限文件扫描与候选文件摘要。",
        "",
        "## 文件名搜索",
        "",
        ...files.slice(0, SUMMARY_FILE_LIMIT).map((file) => `- ${file}`),
        ...(files.length > SUMMARY_FILE_LIMIT
          ? [`- ……其余 ${files.length - SUMMARY_FILE_LIMIT} 个文件已省略`]
          : []),
        "",
        "## 关键字扫描",
        "",
        ...(keywordMatches.length === 0
          ? ["- 未在安全范围内命中常见 service/controller/route/test 关键字。"]
          : keywordMatches),
        "",
        "## 候选文件摘要",
        "",
        ...(candidateSummaries.length === 0
          ? ["- 未找到可安全读取的候选文本文件。"]
          : candidateSummaries),
      ].join("\n"),
      packageStructure: [
        "# 包结构",
        "",
        ...directories.map((dir) => `- ${dir}`),
      ].join("\n"),
      architecture: [
        "# 架构",
        "",
        "由受限的降级文件扫描生成；符号与调用关系不可用。",
        "",
        `发现文件数：${files.length}`,
        `目录数：${directories.length}`,
        "",
        "可优先检查的候选文件：",
        ...(candidateSummaries.length === 0
          ? ["- 无"]
          : candidateSummaries
              .filter((line) => line.startsWith("### "))
              .map((line) => `- ${line.replace(/^### /, "")}`)),
      ].join("\n"),
    };
  }
}

const CODEBASE_MEMORY_MCP_URL =
  "https://github.com/DeusData/codebase-memory-mcp";

function clampConfidence(raw: unknown): number {
  if (typeof raw !== "number" || !Number.isFinite(raw)) return 0.3;
  if (raw >= 1) return 0.99;
  if (raw <= 0) return 0.01;
  return raw;
}

function coerceImpactPayload(input: unknown): ImpactPayload {
  if (input === null || typeof input !== "object")
    return { files: [], symbols: [], tests: [], risks: [] };
  const candidate = input as Partial<ImpactPayload> & {
    files?: unknown;
    symbols?: unknown;
    tests?: unknown;
    risks?: unknown;
  };
  return {
    files: toStringArray(candidate.files),
    symbols: toStringArray(candidate.symbols),
    tests: toStringArray(candidate.tests),
    risks: toStringArray(candidate.risks),
  };
}

function toStringArray(input: unknown): string[] {
  if (!Array.isArray(input)) return [];
  return input
    .filter((value): value is string => typeof value === "string")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

async function scanFiles(root: string, limit = 2_000): Promise<string[]> {
  const absoluteRoot = resolve(root);
  const result: string[] = [];
  const queue = [absoluteRoot];
  while (queue.length > 0 && result.length < limit) {
    const directory = queue.shift();
    if (directory === undefined) break;
    const handle = await opendir(directory);
    for await (const entry of handle) {
      if (entry.name.startsWith(".") && entry.name !== ".github") continue;
      const absolute = resolve(directory, entry.name);
      if (entry.isDirectory()) {
        if (!EXCLUDED_DIRECTORIES.has(entry.name)) queue.push(absolute);
      } else if (entry.isFile()) {
        // 即使只是文件摘要，也要跳过明显敏感文件。
        if (isSecretFile(entry.name)) continue;
        if (isGeneratedMetadataFile(entry.name)) continue;
        result.push(relative(absoluteRoot, absolute).split("\\").join("/"));
        if (result.length >= limit) break;
      }
    }
  }
  return result.sort();
}

function isSecretFile(name: string): boolean {
  const lower = name.toLowerCase();
  return (
    lower === "id_rsa" ||
    lower === "id_ed25519" ||
    lower === "kubeconfig" ||
    lower === "application-prod.yml" ||
    lower === "application-prod.yaml" ||
    lower === "application-prod.properties" ||
    [".pem", ".key", ".p12", ".jks"].some((extension) =>
      lower.endsWith(extension),
    )
  );
}

function isGeneratedMetadataFile(name: string): boolean {
  return name.endsWith(".meta.json");
}

async function collectKeywordMatches(
  root: string,
  files: string[],
): Promise<string[]> {
  const matches: string[] = [];
  for (const rule of KEYWORD_RULES) {
    const hitFiles: string[] = [];
    for (const file of files.slice(0, KEYWORD_FILE_LIMIT)) {
      if (!isSafeTextFile(file)) continue;
      const content = await tryReadText(root, file, 8 * 1024);
      if (
        content !== null &&
        rule.patterns.some((pattern) => content.includes(pattern))
      ) {
        hitFiles.push(file);
      }
      if (hitFiles.length >= 3) break;
    }
    if (hitFiles.length > 0) {
      matches.push(`- ${rule.label}：${hitFiles.join("、")}`);
    }
  }
  return matches;
}

async function readCandidateSummaries(
  root: string,
  files: string[],
): Promise<string[]> {
  const candidates = pickCandidateFiles(files);
  const summaries: string[] = [];
  for (const file of candidates) {
    const content = await tryReadText(root, file, 12 * 1024);
    if (content === null) continue;
    const preview = content
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .slice(0, 6)
      .join(" / ");
    summaries.push(`### ${file}`);
    summaries.push(preview === "" ? "（文件为空）" : preview);
    summaries.push("");
  }
  return summaries;
}

function pickCandidateFiles(files: string[]): string[] {
  const preferred = [
    "README.md",
    "package.json",
    "tsconfig.json",
    "src/index.ts",
    "src/main.ts",
    "src/app.ts",
  ];
  const selected = preferred.filter((file) => files.includes(file));
  for (const file of files) {
    if (selected.length >= CANDIDATE_FILE_LIMIT) break;
    if (selected.includes(file)) continue;
    if (!isSafeTextFile(file)) continue;
    if (
      file.includes("/test") ||
      file.includes(".test.") ||
      file.includes("/spec")
    ) {
      continue;
    }
    selected.push(file);
  }
  return selected;
}

function isSafeTextFile(file: string): boolean {
  const lower = file.toLowerCase();
  return SAFE_TEXT_EXTENSIONS.some((extension) => lower.endsWith(extension));
}

async function tryReadText(
  root: string,
  file: string,
  maxBytes: number,
): Promise<string | null> {
  try {
    const content = await readFile(resolve(root, file), "utf8");
    return content.slice(0, maxBytes);
  } catch {
    return null;
  }
}
