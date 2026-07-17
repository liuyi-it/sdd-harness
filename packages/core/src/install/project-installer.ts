import { mkdir, readFile } from "node:fs/promises";
import { join } from "node:path";

import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import type { AdapterManifest } from "../adapters/types.js";
import { COMMANDS } from "../contracts.js";
import { CANONICAL_SCHEMAS } from "./canonical-schemas.js";

const MANAGED_INSTRUCTION_START = "<!-- sdd-harness:managed -->";
const MANAGED_INSTRUCTION_END = "<!-- sdd-harness:managed:end -->";
const LEGACY_MANAGED_INSTRUCTION_LINES = new Set([
  "## sdd-harness",
  "使用 sdd 命令通过已安装 Adapter 推进工作流。.sdd/ 与 Core CommandResult 是唯一事实源。",
  "不得绕过阶段、范围、锁、验证、审查或归档门禁；阶段工程方法由 policyBundle 渐进加载。",
]);

/**
 * 按选定的适配器清单安装项目集成文件。
 * 每个适配器独立安装其指令文件、commands、skills 和 rules。
 * schemas 与适配器无关，始终安装。
 */
export async function installProjectIntegration(
  root: string,
  manifests: AdapterManifest[],
  _options: { force?: boolean } = {},
): Promise<void> {
  void _options;
  const instructions = [
    ...new Map(
      manifests.map((manifest) => [
        manifest.instructionFile,
        manifest.instructionContent,
      ]),
    ).entries(),
  ];
  await Promise.all([
    ...instructions.map(([instructionFile, instructionContent]) =>
      installInstructions(join(root, instructionFile), instructionContent),
    ),
    ...manifests.flatMap((manifest) => [
      installAdapterCapabilities(root, manifest),
      installCommands(root, manifest),
      ...(manifest.skillsDir !== undefined &&
      manifest.skillContent !== undefined
        ? [installSkill(root, manifest)]
        : []),
    ]),
    installSchemas(root),
  ]);
}

async function installAdapterCapabilities(
  root: string,
  manifest: AdapterManifest,
): Promise<void> {
  const path = join(
    root,
    ".sdd",
    "adapters",
    manifest.agent,
    "capabilities.json",
  );
  await mkdir(join(path, ".."), { recursive: true });
  const content = `${JSON.stringify(
    {
      schemaVersion: "1.0.0",
      agent: manifest.agent,
      capabilities: manifest.capabilities,
      degraded: manifest.degradationReason !== undefined,
      degradationReason: manifest.degradationReason ?? null,
      warnings: manifest.warnings,
    },
    null,
    2,
  )}\n`;
  await new ArtifactWriter().write(path, content, {
    generatedBy: "sdd-harness",
    agent: manifest.agent,
    capabilities: manifest.capabilities,
  });
}

/**
 * 刷新宿主指令文件中的 sdd-harness 受管区块。
 * 用户区块始终保留；旧版只有起始标记的区块会迁移为带结束标记的新格式。
 */
async function installInstructions(
  path: string,
  managedContent: string,
): Promise<void> {
  const writer = new ArtifactWriter();
  const normalizedManagedContent = normalizeManagedContent(managedContent);
  let existing = "";
  try {
    existing = await readFile(path, "utf8");
  } catch {
    // 文件不存在是预期情况（首次 init）
  }
  if (existing === "") {
    await writer.write(
      path,
      normalizedManagedContent,
      managedInputs(normalizedManagedContent),
    );
    return;
  }

  const refreshed = refreshManagedInstruction(
    existing,
    normalizedManagedContent,
  );
  if (refreshed === existing) {
    await ensureMetadata(path, managedInputs(normalizedManagedContent));
    return;
  }
  await writer.write(path, refreshed, managedInputs(normalizedManagedContent));
}

function normalizeManagedContent(content: string): string {
  const withoutEnd = content
    .split("\n")
    .filter((line) => line.trim() !== MANAGED_INSTRUCTION_END)
    .join("\n")
    .trimEnd();
  return `${withoutEnd}\n${MANAGED_INSTRUCTION_END}\n`;
}

function refreshManagedInstruction(
  existing: string,
  managedContent: string,
): string {
  const start = existing.indexOf(MANAGED_INSTRUCTION_START);
  if (start < 0) return `${existing.trimEnd()}\n\n${managedContent}`;

  const end = existing.indexOf(
    MANAGED_INSTRUCTION_END,
    start + MANAGED_INSTRUCTION_START.length,
  );
  if (end >= 0) {
    const suffix = existing.slice(end + MANAGED_INSTRUCTION_END.length);
    return `${existing.slice(0, start)}${managedContent.trimEnd()}${suffix}`;
  }

  const managedLines = new Set(
    managedContent
      .split("\n")
      .map((line) => line.trimEnd())
      .filter(Boolean),
  );
  for (const line of LEGACY_MANAGED_INSTRUCTION_LINES) managedLines.add(line);
  const preservedLegacyTail = existing
    .slice(start + MANAGED_INSTRUCTION_START.length)
    .split("\n")
    .filter((line) => !managedLines.has(line.trimEnd()))
    .join("\n")
    .replace(/^\n+/, "\n");
  return `${existing.slice(0, start)}${managedContent.trimEnd()}${preservedLegacyTail}`;
}

/**
 * 按 manifest 的 commandsDir 和 commandTemplate 生成所有 sdd 命令文件。
 */
async function installCommands(
  root: string,
  manifest: AdapterManifest,
): Promise<void> {
  const directory = join(root, manifest.commandsDir);
  await mkdir(directory, { recursive: true });
  const writer = new ArtifactWriter();
  const inputs = managedInputs(manifest.commandTemplate);
  await Promise.all(
    COMMANDS.map((command) =>
      writer.write(
        join(directory, `sdd.${command}.md`),
        manifest.commandTemplate.replaceAll("{command}", command),
        inputs,
      ),
    ),
  );
}

/**
 * 按 manifest 的 skillsDir 安装 SKILL.md。
 */
async function installSkill(
  root: string,
  manifest: AdapterManifest,
): Promise<void> {
  const skillPath = join(root, manifest.skillsDir!, "SKILL.md");
  await mkdir(join(skillPath, ".."), { recursive: true });
  await new ArtifactWriter().write(
    skillPath,
    manifest.skillContent!,
    managedInputs(manifest.skillContent!),
  );
}

/**
 * 安装 JSON Schema 文件（与适配器无关，始终安装）。
 */
async function installSchemas(root: string): Promise<void> {
  const directory = join(root, ".sdd", "schemas");
  await mkdir(directory, { recursive: true });
  const writer = new ArtifactWriter();
  await Promise.all(
    Object.entries(CANONICAL_SCHEMAS).map(async ([name, content]) =>
      writer.write(join(directory, name), content, managedInputs(content)),
    ),
  );
}

function managedInputs(content: string): Record<string, unknown> {
  return {
    generatedBy: "sdd-harness",
    content,
  };
}

async function ensureMetadata(
  path: string,
  inputs: Record<string, unknown>,
): Promise<void> {
  const writer = new ArtifactWriter();
  if ((await writer.metadata(path)) === undefined) {
    await writer.write(path, await readFile(path, "utf8"), inputs);
  }
}
