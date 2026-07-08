import { access, mkdir, readFile } from "node:fs/promises";
import { basename, join } from "node:path";

import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import type { AdapterManifest } from "../adapters/types.js";
import { COMMANDS } from "../contracts.js";
import { CANONICAL_SCHEMAS } from "./canonical-schemas.js";

export interface ProjectIntegrationResult {
  candidateFiles: string[];
}

/**
 * 按选定的适配器清单安装项目集成文件。
 * 每个适配器独立安装其指令文件、commands、skills 和 rules。
 * schemas 与适配器无关，始终安装。
 */
export async function installProjectIntegration(
  root: string,
  manifests: AdapterManifest[],
  options: { force?: boolean } = {},
): Promise<ProjectIntegrationResult> {
  const results = await Promise.all([
    ...manifests.flatMap((manifest) => [
      installInstructions(
        join(root, manifest.instructionFile),
        manifest.instructionContent,
        options,
      ),
      installCommands(root, manifest, options),
      ...(manifest.skillsDir !== undefined &&
      manifest.skillContent !== undefined
        ? [installSkill(root, manifest, options)]
        : []),
      ...(manifest.rules !== undefined
        ? manifest.rules.map((rule) =>
            installRule(root, rule.file, rule.content, options),
          )
        : []),
    ]),
    installSchemas(root, options),
  ]);
  return { candidateFiles: results.flatMap((result) => result.candidateFiles) };
}

/**
 * 行级去重追加指令文件内容。
 * 文件不存在 → 创建；文件存在 → 仅追加不存在的行。
 * force=true 时全量覆盖。
 */
async function installInstructions(
  path: string,
  managedContent: string,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const writer = new ArtifactWriter();
  let existing = "";
  try {
    existing = await readFile(path, "utf8");
  } catch {
    // 文件不存在是预期情况（首次 init）
  }
  if (existing === "") {
    await writer.write(path, managedContent, managedInputs(managedContent));
    return { candidateFiles: [] };
  }

  // 逐行比较：只追加 existing 中不存在的行，避免重复内容
  const existingLines = new Set(
    existing.split("\n").map((line) => line.trimEnd()),
  );
  const newLines = managedContent
    .split("\n")
    .filter((line) => !existingLines.has(line.trimEnd()));

  if (newLines.length === 0) {
    await ensureMetadata(path, managedInputs(managedContent));
    return { candidateFiles: [] };
  }

  if (options.force === true) {
    await writer.write(path, managedContent, managedInputs(managedContent));
    return { candidateFiles: [] };
  }

  // 追加新行到文件末尾
  const appendContent = `\n${newLines.join("\n")}`;
  await writer.write(
    path,
    existing + appendContent,
    managedInputs(managedContent),
  );
  return { candidateFiles: [] };
}

/**
 * 按 manifest 的 commandsDir 和 commandTemplate 生成所有 sdd 命令文件。
 */
async function installCommands(
  root: string,
  manifest: AdapterManifest,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const directory = join(root, manifest.commandsDir);
  await mkdir(directory, { recursive: true });
  const results = await Promise.all(
    COMMANDS.map((command) =>
      writeManagedFile(
        join(directory, `sdd.${command}.md`),
        manifest.commandTemplate.replaceAll("{command}", command),
        options,
      ),
    ),
  );
  return { candidateFiles: results.flatMap((result) => result.candidateFiles) };
}

/**
 * 按 manifest 的 skillsDir 安装 SKILL.md。
 */
async function installSkill(
  root: string,
  manifest: AdapterManifest,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const skillPath = join(root, manifest.skillsDir!, "SKILL.md");
  await mkdir(join(skillPath, ".."), { recursive: true });
  return writeManagedFile(skillPath, manifest.skillContent!, options);
}

/**
 * 安装单个 rule 文件。
 */
async function installRule(
  root: string,
  ruleFile: string,
  content: string,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const rulePath = join(root, ruleFile);
  await mkdir(join(rulePath, ".."), { recursive: true });
  return writeManagedFile(rulePath, content, options);
}

/**
 * 安装 JSON Schema 文件（与适配器无关，始终安装）。
 */
async function installSchemas(
  root: string,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const directory = join(root, ".sdd", "schemas");
  await mkdir(directory, { recursive: true });
  const results = await Promise.all(
    Object.entries(CANONICAL_SCHEMAS).map(async ([name, content]) =>
      writeManagedFile(join(directory, name), content, options),
    ),
  );
  return { candidateFiles: results.flatMap((result) => result.candidateFiles) };
}

async function writeManagedFile(
  path: string,
  content: string,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const writer = new ArtifactWriter();
  try {
    const existing = await readFile(path, "utf8");
    if (existing === content) {
      await ensureMetadata(path, managedInputs(content));
      return { candidateFiles: [] };
    }
    if (options.force === true) {
      await writer.write(path, content, managedInputs(content));
      return { candidateFiles: [] };
    }
    const candidatePath = `${path}.candidate.md`;
    await writer.write(candidatePath, content, managedInputs(content));
    return { candidateFiles: [basename(candidatePath)] };
  } catch {
    await writer.write(path, content, managedInputs(content));
    return { candidateFiles: [] };
  }
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
  try {
    await access(`${path}.meta.json`);
  } catch {
    await new ArtifactWriter().write(
      path,
      await readFile(path, "utf8"),
      inputs,
    );
  }
}
