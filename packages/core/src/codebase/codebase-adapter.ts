import { opendir } from "node:fs/promises";
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

export interface CodebaseResult extends CodebaseSummary {
  provider: "codebase-memory-mcp" | "fallback-file-scan";
  degraded: boolean;
  reason?: string;
}

export interface McpTransport {
  isAvailable(): Promise<boolean>;
  index(root: string): Promise<void>;
  summarize(root: string): Promise<CodebaseSummary>;
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

export class CodebaseAdapter {
  constructor(private readonly transport?: McpTransport) {}

  async initialize(root: string): Promise<CodebaseResult> {
    if (this.transport !== undefined && (await this.transport.isAvailable())) {
      await this.transport.index(root);
      return {
        provider: "codebase-memory-mcp",
        degraded: false,
        ...(await this.transport.summarize(root)),
      };
    }
    // 降级模式只扫描路径和目录结构，不读取正文，避免把仓库内容注入上下文。
    const files = await scanFiles(root);
    const directories = [
      ...new Set(files.map((file) => file.split("/").slice(0, -1).join("/"))),
    ]
      .filter(Boolean)
      .sort();
    return {
      provider: "fallback-file-scan",
      degraded: true,
      reason: "codebase-memory-mcp 不可用",
      codebaseSummary: [
        "# 代码库摘要",
        "",
        ...files.map((file) => `- ${file}`),
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
      ].join("\n"),
    };
  }
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
