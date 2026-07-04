import { opendir, readFile } from "node:fs/promises";
import { relative, resolve } from "node:path";

/**
 * CodebaseAdapter 封装两类代码上下文来源：
 * 1. 优先使用 codebase-memory-mcp
 * 2. 不可用时退回受限文件扫描
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
      reason: MCP_UNAVAILABLE_REASON,
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
