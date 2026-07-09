import { access, mkdir, readFile } from "node:fs/promises";
import { join } from "node:path";

import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import type { AdapterManifest } from "../adapters/types.js";
import { COMMANDS } from "../contracts.js";
import { CANONICAL_SCHEMAS } from "./canonical-schemas.js";

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
  await Promise.all([
    ...manifests.flatMap((manifest) => [
      installInstructions(
        join(root, manifest.instructionFile),
        manifest.instructionContent,
        _options,
      ),
      installCommands(root, manifest),
      ...(manifest.skillsDir !== undefined &&
      manifest.skillContent !== undefined
        ? [installSkill(root, manifest)]
        : []),
      ...(manifest.rules !== undefined
        ? manifest.rules.map((rule) =>
            installRule(root, rule.file, rule.content),
          )
        : []),
    ]),
    installSchemas(root),
  ]);
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
): Promise<void> {
  const writer = new ArtifactWriter();
  let existing = "";
  try {
    existing = await readFile(path, "utf8");
  } catch {
    // 文件不存在是预期情况（首次 init）
  }
  if (existing === "") {
    await writer.write(path, managedContent, managedInputs(managedContent));
    return;
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
    return;
  }

  if (options.force === true) {
    await writer.write(path, managedContent, managedInputs(managedContent));
    return;
  }

  // 追加新行到文件末尾
  const appendContent = `\n${newLines.join("\n")}`;
  await writer.write(
    path,
    existing + appendContent,
    managedInputs(managedContent),
  );
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
 * 安装单个 rule 文件。
 */
async function installRule(
  root: string,
  ruleFile: string,
  content: string,
): Promise<void> {
  const rulePath = join(root, ruleFile);
  await mkdir(join(rulePath, ".."), { recursive: true });
  await new ArtifactWriter().write(rulePath, content, managedInputs(content));
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
