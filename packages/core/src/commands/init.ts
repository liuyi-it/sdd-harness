import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { parse, stringify } from "yaml";
import { z } from "zod";

import { AuditLogger } from "../audit/audit-logger.js";
import { type CodebaseAdapter } from "../codebase/codebase-adapter.js";
import { type CommandResult } from "../contracts.js";
import { PINNED_DEPENDENCIES } from "../dependencies.js";
import { SddError } from "../errors.js";
import { FileLock } from "../state/file-lock.js";
import { createInitialState, StateStore } from "../state/state-store.js";
import { installProjectIntegration } from "../install/project-installer.js";

/**
 * init 负责创建 `.sdd/` 基础目录、安装宿主集成文件，并初始化代码库索引。
 * 它是整个仓库进入受控工作流的入口。
 */
const REQUIRED_DIRECTORIES = [
  "index",
  "changes",
  "context-packs",
  "runs",
  "logs",
  "plugins",
  "adapters",
  "schemas",
] as const;

const configSchema = z
  .object({
    schemaVersion: z.literal("1.0.0"),
    project: z.object({ name: z.string().min(1) }).passthrough(),
    plugins: z.object({}).passthrough(),
    codebase: z.object({}).passthrough(),
    workflow: z.object({}).passthrough(),
    quality: z.object({}).passthrough(),
    security: z.object({}).passthrough(),
  })
  .passthrough();

const REQUIRED_CONFIG_KEYS = [
  "schemaVersion",
  "project",
  "plugins",
  "codebase",
  "workflow",
  "quality",
  "security",
] as const;

export async function runInit(
  root: string,
  codebase: CodebaseAdapter,
): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd init");
  try {
    const sddRoot = join(root, ".sdd");
    await Promise.all(
      REQUIRED_DIRECTORIES.map((directory) =>
        mkdir(join(sddRoot, directory), { recursive: true }),
      ),
    );
    const store = new StateStore(root);
    if (!(await exists(store.path))) await store.write(createInitialState());
    await store.update((state) => ({
      ...state,
      currentPhase: "INITIALIZING",
      inProgressPhase: "INITIALIZING",
      lastCommand: "sdd init",
      lastError: null,
    }));
    await writeIfMissing(
      join(sddRoot, "config.yml"),
      stringify(defaultConfig(root)),
    );
    const configWarnings = await validateConfig(join(sddRoot, "config.yml"));
    const integration = await installProjectIntegration(root);
    await store.update((state) => ({
      ...state,
      currentPhase: "INDEXING",
      inProgressPhase: "INDEXING",
      indexStatus: "INDEXING",
    }));

    const index = await codebase.initialize(root);
    await writeFile(
      join(sddRoot, "index", "codebase-summary.md"),
      `${index.codebaseSummary}\n`,
      "utf8",
    );
    await writeFile(
      join(sddRoot, "index", "package-structure.md"),
      `${index.packageStructure}\n`,
      "utf8",
    );
    await writeFile(
      join(sddRoot, "index", "architecture.md"),
      `${index.architecture}\n`,
      "utf8",
    );
    await writeDependencyMetadata(sddRoot, index.provider);

    const ready = await store.update((state) => ({
      ...state,
      initialized: true,
      currentPhase: "INDEX_READY",
      previousPhase: "NOT_INITIALIZED",
      inProgressPhase: null,
      indexStatus: "INDEX_READY",
      codebaseProvider: index.provider,
      degraded: index.degraded,
      degradedReason: index.reason ?? null,
      suggestedCommand: "sdd new",
    }));
    await new AuditLogger(root).write({
      command: "sdd init",
      phase: ready.currentPhase,
      result: "PASS",
      ...(index.degraded ? { message: index.reason } : {}),
    });
    return {
      ok: true,
      state: ready.currentPhase,
      exitCode: 0,
      next: "sdd new",
      ...buildWarnings(index, integration.candidateFiles, configWarnings),
    };
  } finally {
    await lock.release();
  }
}

function defaultConfig(root: string): Record<string, unknown> {
  return {
    schemaVersion: "1.0.0",
    project: {
      name: root.split(/[\\/]/).filter(Boolean).at(-1) ?? "auto-detect",
    },
    plugins: { claudeCode: { enabled: true }, codex: { enabled: true } },
    codebase: {
      provider: "codebase-memory-mcp",
      fallbackProvider: "file-scan",
      autoIndexOnInit: true,
    },
    workflow: {
      maxClarifyingQuestionsPerRound: 5,
      requireBlockerAnswers: true,
      stopOnFailure: true,
    },
    quality: { requireFileScopeCheck: true, requireDriftCheck: true },
    contextPack: { maxSizeKb: 30 },
    audit: { maxSizeMb: 10, maxFiles: 5 },
    git: { createBranch: false, createWorktree: false },
    security: {
      blockOutsideRepo: true,
      blockSymlinksOutsideRepo: true,
      redactSecretsInLogs: true,
    },
  };
}

async function writeDependencyMetadata(
  sddRoot: string,
  codebaseProvider: "codebase-memory-mcp" | "fallback-file-scan",
): Promise<void> {
  const entries = [
    ["codebase-memory-mcp", PINNED_DEPENDENCIES.codebaseMemoryMcp],
    ["openspec", PINNED_DEPENDENCIES.openSpec],
    ["superpowers", PINNED_DEPENDENCIES.superpowers],
  ] as const;
  await Promise.all(
    entries.map(async ([name, metadata]) => {
      const directory = join(sddRoot, "adapters", name);
      await mkdir(directory, { recursive: true });
      await writeFile(
        join(directory, "version.json"),
        `${JSON.stringify(
          {
            ...metadata,
            name,
            resolvedAt: new Date().toISOString(),
            status:
              name === "codebase-memory-mcp"
                ? codebaseProvider === "codebase-memory-mcp"
                  ? "available"
                  : "unavailable"
                : "available",
          },
          null,
          2,
        )}\n`,
        "utf8",
      );
    }),
  );
}

async function writeIfMissing(path: string, content: string): Promise<void> {
  if (!(await exists(path))) await writeFile(path, content, "utf8");
}

function buildWarnings(
  index: { degraded: boolean; reason?: string | null },
  candidateFiles: string[],
  configWarnings: string[],
): { warnings?: string[] } {
  const warnings: string[] = [];
  if (index.degraded) {
    warnings.push(`降级模式：${index.reason ?? "MCP 不可用"}`);
  }
  if (candidateFiles.length > 0) {
    warnings.push(
      `检测到人工修改，已生成候选文件供人工合并：${candidateFiles.join(", ")}`,
    );
  }
  warnings.push(...configWarnings);
  return warnings.length === 0 ? {} : { warnings };
}

async function validateConfig(path: string): Promise<string[]> {
  const raw = await readFile(path, "utf8");
  let parsed: unknown;
  try {
    parsed = parse(raw);
  } catch (error) {
    throw new SddError(
      "E_STATE_CORRUPTED",
      `config.yml 解析失败：${error instanceof Error ? error.message : String(error)}`,
      "sdd init",
    );
  }
  const config = configSchema.safeParse(parsed);
  if (!config.success) {
    throw new SddError(
      "E_STATE_CORRUPTED",
      `config.yml 校验失败：${config.error.issues.map((issue) => issue.path.join(".") || issue.message).join("; ")}`,
      "sdd init",
    );
  }
  const unknownKeys = Object.keys(config.data).filter(
    (key) =>
      !REQUIRED_CONFIG_KEYS.includes(
        key as (typeof REQUIRED_CONFIG_KEYS)[number],
      ),
  );
  return unknownKeys.length === 0
    ? []
    : [`config.yml 包含未知字段，已保留原值：${unknownKeys.join(", ")}`];
}

async function exists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}
