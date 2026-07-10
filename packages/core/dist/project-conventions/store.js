import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
export class ProjectConventionsStore {
    directory;
    jsonPath;
    markdownPath;
    constructor(root) {
        this.directory = join(root, ".sdd", "project");
        this.jsonPath = join(this.directory, "conventions.json");
        this.markdownPath = join(this.directory, "conventions.md");
    }
    async read() {
        try {
            return JSON.parse(await readFile(this.jsonPath, "utf8"));
        }
        catch {
            return null;
        }
    }
    async write(profile) {
        const existing = await this.read();
        if (existing?.strategy === "user-defined" &&
            profile.strategy !== "user-defined") {
            return existing;
        }
        await mkdir(this.directory, { recursive: true });
        await writeFile(this.jsonPath, `${JSON.stringify(profile, null, 2)}\n`, "utf8");
        await writeFile(this.markdownPath, renderMarkdown(profile), "utf8");
        return profile;
    }
}
function renderMarkdown(profile) {
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
            : profile.conventions.map((entry) => `- ${entry.kind}: ${entry.value} (${entry.evidence.join(", ")})`)),
        "",
        "## 未知项",
        "",
        ...(profile.unknowns.length === 0
            ? ["- 无"]
            : profile.unknowns.map((item) => `- ${item}`)),
    ].join("\n");
}
function renderList(values) {
    return values.length === 0 ? "无" : values.join(", ");
}
//# sourceMappingURL=store.js.map