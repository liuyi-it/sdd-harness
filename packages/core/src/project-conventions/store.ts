import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import type { ProjectConventionProfile } from "./model.js";

export class ProjectConventionsStore {
  readonly directory: string;
  readonly jsonPath: string;
  readonly markdownPath: string;

  constructor(root: string) {
    this.directory = join(root, ".sdd", "project");
    this.jsonPath = join(this.directory, "conventions.json");
    this.markdownPath = join(this.directory, "conventions.md");
  }

  async read(): Promise<ProjectConventionProfile | null> {
    try {
      return JSON.parse(
        await readFile(this.jsonPath, "utf8"),
      ) as ProjectConventionProfile;
    } catch {
      return null;
    }
  }

  async write(
    profile: ProjectConventionProfile,
  ): Promise<ProjectConventionProfile> {
    const existing = await this.read();
    if (
      existing?.strategy === "user-defined" &&
      profile.strategy !== "user-defined"
    ) {
      return existing;
    }
    await mkdir(this.directory, { recursive: true });
    await writeFile(
      this.jsonPath,
      `${JSON.stringify(profile, null, 2)}\n`,
      "utf8",
    );
    await writeFile(this.markdownPath, renderMarkdown(profile), "utf8");
    return profile;
  }
}

function renderMarkdown(profile: ProjectConventionProfile): string {
  return [
    "# 项目规范画像",
    "",
    `- schemaVersion: ${profile.schemaVersion}`,
    `- projectType: ${profile.projectType}`,
    `- strategy: ${profile.strategy}`,
    "",
    "## 目录",
    "",
    `- source: ${renderList(profile.directories.source)}`,
    `- test: ${renderList(profile.directories.test)}`,
    `- assets: ${renderList(profile.directories.assets)}`,
    `- config: ${renderList(profile.directories.config)}`,
    "",
    "## 约定",
    "",
    ...(profile.conventions.length === 0
      ? ["- 无"]
      : profile.conventions.map(
          (entry) =>
            `- ${entry.kind}: ${entry.value} (${entry.evidence.join(", ")})`,
        )),
    "",
    "## 未知项",
    "",
    ...(profile.unknowns.length === 0
      ? ["- 无"]
      : profile.unknowns.map((item) => `- ${item}`)),
  ].join("\n");
}

function renderList(values: string[]): string {
  return values.length === 0 ? "无" : values.join(", ");
}
