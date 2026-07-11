import {
  access,
  appendFile,
  copyFile,
  mkdir,
  readFile,
  writeFile,
} from "node:fs/promises";
import { join } from "node:path";

import { parse, stringify } from "yaml";
import { z } from "zod";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { type CodebaseAdapter } from "../codebase/codebase-adapter.js";
import { type CommandResult } from "../contracts.js";
import { PINNED_DEPENDENCIES } from "../dependencies.js";
import {
  decodeArtifactContent,
  resolveCodebaseMemoryArtifactName,
  verifyChecksumManifest,
  type ComponentIntegrityEvidence,
} from "../dependency-integrity.js";
import { SddError } from "../errors.js";
import { FileLock } from "../state/file-lock.js";
import {
  CURRENT_SCHEMA_VERSION,
  migrateConfigDocument,
} from "../state/schema-migration.js";
import { createInitialState, StateStore } from "../state/state-store.js";
import { installProjectIntegration } from "../install/project-installer.js";
import { createDefaultLoopSpec } from "../loop/loop-spec.js";
import { LoopStore } from "../loop/loop-store.js";
import {
  createEmptyProjectProfile,
  discoverProjectConventions,
  isEmptyProject,
} from "../project-conventions/scanner.js";
import { ProjectConventionsStore } from "../project-conventions/store.js";
import {
  assertRecoverableCommandState,
  normalizeCommandError,
  persistCommandFailure,
} from "./recovery.js";
import { getAvailableAdapters } from "../adapters/registry.js";
import type { AdapterManifest } from "../adapters/types.js";
import { timeoutMilliseconds, withTimeout } from "./timeout.js";

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
  "project",
  "loop",
] as const;

const configSchema = z
  .object({
    schemaVersion: z.literal(CURRENT_SCHEMA_VERSION),
    project: z.object({ name: z.string().min(1) }).passthrough(),
    plugins: z.object({}).passthrough(),
    codebase: z.object({}).passthrough(),
    workflow: z.object({}).passthrough(),
    quality: z.object({}).passthrough(),
    security: z.object({}).passthrough(),
    contextPack: z.object({}).passthrough(),
    audit: z.object({}).passthrough(),
    git: z.object({}).passthrough(),
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
  "contextPack",
  "audit",
  "git",
] as const;

export async function runInit(
  root: string,
  codebase: CodebaseAdapter,
  args?: Record<string, unknown>,
  signal?: AbortSignal,
): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd init", undefined, lockOptions(args));
  const store = new StateStore(root);
  let started = false;
  let inProgressPhase: CommandResult["state"] = "INITIALIZING";
  try {
    if (await exists(store.path)) {
      assertRecoverableCommandState(await store.read(), "sdd init");
    }
    const sddRoot = join(root, ".sdd");
    await Promise.all(
      REQUIRED_DIRECTORIES.map((directory) =>
        mkdir(join(sddRoot, directory), { recursive: true }),
      ),
    );
    if (!(await exists(store.path))) await store.write(createInitialState());
    await store.update((state) => ({
      ...state,
      currentPhase: "INITIALIZING",
      inProgressPhase: "INITIALIZING",
      lastCommand: "sdd init",
      lastError: null,
    }));
    started = true;
    const selectedAgents = normalizeAgentArg(args?.agent);
    const allManifests = await getAvailableAdapters();
    const manifests = filterManifests(allManifests, selectedAgents);
    const writer = new ArtifactWriter();
    await writer.write(
      join(sddRoot, "config.yml"),
      stringify(
        defaultConfig(
          root,
          manifests.map((m) => m.agent),
        ),
      ),
      { generatedBy: "sdd-harness", purpose: "config" },
    );
    await migrateConfigIfNeeded(root, join(sddRoot, "config.yml"));
    const configWarnings = await validateConfig(join(sddRoot, "config.yml"));
    await installProjectIntegration(root, manifests, {
      force: args?.force === true,
    });
    inProgressPhase = "INDEXING";
    await store.update((state) => ({
      ...state,
      currentPhase: "INDEXING",
      inProgressPhase: "INDEXING",
      indexStatus: "INDEXING",
    }));

    const index = await withTimeout(
      codebase.initialize(root),
      timeoutMilliseconds(args),
      "sdd init",
      signal,
    );
    const indexInputs = {
      provider: index.provider,
      degraded: index.degraded,
      reason: index.reason ?? null,
      diagnostics: index.diagnostics,
    };
    await writer.write(
      join(sddRoot, "index", "codebase-summary.md"),
      index.codebaseSummary,
      indexInputs,
    );
    await writer.write(
      join(sddRoot, "index", "package-structure.md"),
      index.packageStructure,
      indexInputs,
    );
    await writer.write(
      join(sddRoot, "index", "architecture.md"),
      index.architecture,
      indexInputs,
    );
    await writeFile(
      join(sddRoot, "index", "codebase-diagnostics.json"),
      `${JSON.stringify(index.diagnostics, null, 2)}\n`,
      "utf8",
    );
    const capabilities = await codebase.capabilities(root);
    await writeFile(
      join(sddRoot, "index", "mcp-capabilities.json"),
      `${JSON.stringify(capabilities, null, 2)}\n`,
      "utf8",
    );
    await verifyDependencyIntegrityIfProvided(sddRoot, args);
    await writeDependencyMetadata(sddRoot, index.provider);
    const loopStore = new LoopStore(root);
    await loopStore.writeSpec(createDefaultLoopSpec());
    const conventionsStore = new ProjectConventionsStore(root);
    const emptyProject = await isEmptyProject(root);
    const structurePolicy = readStructurePolicy(args);
    if (emptyProject && structurePolicy === undefined) {
      const clarifying = await store.update((state) => ({
        ...state,
        initialized: true,
        currentPhase: "CLARIFYING",
        previousPhase: "NOT_INITIALIZED",
        inProgressPhase: null,
        indexStatus: "INDEX_READY",
        codebaseProvider: index.provider,
        degraded: index.degraded,
        degradedReason: index.reason ?? null,
        suggestedCommand: "sdd init",
      }));
      await new AuditLogger(root).write({
        command: "sdd init",
        phase: clarifying.currentPhase,
        result: "PAUSED",
        message: "空项目需要先确认目录结构策略",
      });
      return {
        ok: true,
        state: clarifying.currentPhase,
        exitCode: 0,
        next: "sdd init",
        warnings: [
          "空项目需要先通过 structurePolicy 指定目录结构策略，可选 free-design 或 user-defined",
        ],
      };
    }
    await conventionsStore.write(
      emptyProject
        ? createEmptyProjectProfile(root, structurePolicy ?? "free-design")
        : await discoverProjectConventions(root),
    );

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
      ...buildWarnings(index, configWarnings),
    };
  } catch (error) {
    const normalized = normalizeCommandError(
      error,
      "E_STATE_CORRUPTED",
      "sdd init",
    );
    if (started) {
      await persistCommandFailure(store, normalized, {
        command: "sdd init",
        previousPhase: "NOT_INITIALIZED",
        inProgressPhase,
      });
    }
    throw normalized;
  } finally {
    await lock.release();
  }
}

function defaultConfig(
  root: string,
  agentNames?: string[],
): Record<string, unknown> {
  const plugins: Record<string, unknown> = {};
  if (agentNames === undefined || agentNames.length === 0) {
    // 向后兼容默认值
    plugins.claudeCode = { enabled: true };
    plugins.codex = { enabled: true };
  } else {
    for (const name of agentNames) {
      if (name === "claude") plugins.claudeCode = { enabled: true };
      else if (name === "codex") plugins.codex = { enabled: true };
      else if (name === "opencode") plugins.openCode = { enabled: true };
      else plugins[name] = { enabled: true };
    }
  }
  return {
    schemaVersion: CURRENT_SCHEMA_VERSION,
    project: {
      name: root.split(/[\\/]/).filter(Boolean).at(-1) ?? "auto-detect",
    },
    plugins,
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

async function verifyDependencyIntegrityIfProvided(
  sddRoot: string,
  args: Record<string, unknown> | undefined,
): Promise<void> {
  const evidence = readIntegrityEvidence(args);
  if (evidence === undefined) {
    await writeIntegrityReport(sddRoot, {
      component: "codebase-memory-mcp",
      status: "skipped",
      reason: "未提供可校验的组件安装材料",
    });
    return;
  }
  verifyChecksumManifest({
    manifestContent: evidence.manifestContent,
    expectedManifestSha256:
      evidence.expectedManifestSha256 ??
      PINNED_DEPENDENCIES.codebaseMemoryMcp.checksumManifestSha256,
    artifactName:
      evidence.artifactName ??
      resolveCodebaseMemoryArtifactName(targetDescriptor(evidence)),
    artifactContent: decodeArtifactContent(evidence.artifactContentBase64),
  });
  await writeIntegrityReport(sddRoot, {
    component: "codebase-memory-mcp",
    status: "verified",
    artifactName:
      evidence.artifactName ??
      resolveCodebaseMemoryArtifactName(targetDescriptor(evidence)),
    verifiedAt: new Date().toISOString(),
  });
}

function readIntegrityEvidence(
  args: Record<string, unknown> | undefined,
): ComponentIntegrityEvidence | undefined {
  const integrity = args?.integrity;
  if (
    integrity === undefined ||
    integrity === null ||
    typeof integrity !== "object"
  ) {
    return undefined;
  }
  const codebaseMemoryMcp = (integrity as Record<string, unknown>)
    .codebaseMemoryMcp;
  if (
    codebaseMemoryMcp === undefined ||
    codebaseMemoryMcp === null ||
    typeof codebaseMemoryMcp !== "object"
  ) {
    return undefined;
  }
  const input = codebaseMemoryMcp as Record<string, unknown>;
  if (
    typeof input.manifestContent !== "string" ||
    typeof input.artifactContentBase64 !== "string"
  ) {
    throw new SddError(
      "E_COMPONENT_INTEGRITY_FAILED",
      "组件完整性校验材料格式不正确",
    );
  }
  return {
    manifestContent: input.manifestContent,
    ...(typeof input.expectedManifestSha256 === "string"
      ? { expectedManifestSha256: input.expectedManifestSha256 }
      : {}),
    ...(typeof input.artifactName === "string"
      ? { artifactName: input.artifactName }
      : {}),
    ...(input.targetPlatform === "darwin" || input.targetPlatform === "win32"
      ? { targetPlatform: input.targetPlatform }
      : {}),
    ...(input.targetArch === "arm64" || input.targetArch === "x64"
      ? { targetArch: input.targetArch }
      : {}),
    artifactContentBase64: input.artifactContentBase64,
  };
}

async function writeIntegrityReport(
  sddRoot: string,
  report: Record<string, unknown>,
): Promise<void> {
  const directory = join(sddRoot, "adapters", "codebase-memory-mcp");
  await mkdir(directory, { recursive: true });
  await writeFile(
    join(directory, "integrity.json"),
    `${JSON.stringify(report, null, 2)}\n`,
    "utf8",
  );
}

function targetDescriptor(evidence: ComponentIntegrityEvidence): {
  platform?: string;
  arch?: string;
} {
  return {
    ...(evidence.targetPlatform === undefined
      ? {}
      : { platform: evidence.targetPlatform }),
    ...(evidence.targetArch === undefined ? {} : { arch: evidence.targetArch }),
  };
}

function buildWarnings(
  index: { degraded: boolean; reason?: string | null },
  configWarnings: string[],
): { warnings?: string[] } {
  const warnings: string[] = [];
  if (index.degraded) {
    warnings.push(
      "降级模式：codebase-memory-mcp 当前不可用，已切换为受限文件扫描",
    );
    warnings.push(
      `安装建议：请先安装并配置 codebase-memory-mcp，官方项目地址：${PINNED_DEPENDENCIES.codebaseMemoryMcp.repository}`,
    );
  }
  warnings.push(...configWarnings);
  return warnings.length === 0 ? {} : { warnings };
}

function readStructurePolicy(
  args: Record<string, unknown> | undefined,
): "free-design" | "user-defined" | undefined {
  return args?.structurePolicy === "free-design" ||
    args?.structurePolicy === "user-defined"
    ? args.structurePolicy
    : undefined;
}

function lockOptions(args: Record<string, unknown> | undefined): {
  timeoutMs?: number;
} {
  const timeoutMs = timeoutMilliseconds(args);
  return timeoutMs === undefined ? {} : { timeoutMs };
}

/**
 * 将 args.agent 规范化为字符串数组。
 * 支持逗号分隔字符串 "claude,codex" 或 string[]。
 * 未提供时返回 undefined（表示安装全部可用适配器，保持向后兼容）。
 */
function normalizeAgentArg(raw: unknown): string[] | undefined {
  if (raw === undefined || raw === null) return undefined;
  if (Array.isArray(raw))
    return raw.filter(
      (a): a is string => typeof a === "string" && a.length > 0,
    );
  if (typeof raw === "string")
    return raw
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
  return undefined;
}

/**
 * 按用户选择的 agent 列表过滤 manifest。
 * selectedAgents 为 undefined 时返回全部（向后兼容直接调用 Core API）。
 */
function filterManifests(
  all: AdapterManifest[],
  selected: string[] | undefined,
): AdapterManifest[] {
  if (selected === undefined) return all;
  const set = new Set(selected);
  return all.filter((m) => set.has(m.agent));
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

async function migrateConfigIfNeeded(
  root: string,
  path: string,
): Promise<void> {
  const raw = parse(await readFile(path, "utf8"));
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) {
    throw new SddError(
      "E_STATE_CORRUPTED",
      "config.yml 必须是对象",
      "sdd init",
    );
  }
  const document = raw as Record<string, unknown>;
  if (document.schemaVersion === CURRENT_SCHEMA_VERSION) return;
  const migrated = migrateConfigDocument(document);
  await copyFile(path, `${path}.migration.bak`);
  await writeFile(path, stringify(migrated), "utf8");
  await mkdir(join(root, ".sdd", "logs"), { recursive: true });
  await appendFile(
    join(root, ".sdd", "logs", "migration.log"),
    `${new Date().toISOString()} 1.0.0 -> 1.3.0 (config.yml)\n`,
    "utf8",
  );
}

async function exists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}
