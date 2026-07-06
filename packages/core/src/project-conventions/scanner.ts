import { createHash } from "node:crypto";
import { readdir, readFile, stat } from "node:fs/promises";
import { basename, dirname, join, relative } from "node:path";

import type { ProjectConventionProfile } from "./model.js";

const IGNORED_DIRECTORIES = new Set([
  ".git",
  ".sdd",
  "node_modules",
  ".worktrees",
]);
const DOCUMENT_FILE = /^(readme|license|changelog)(\..+)?$/i;
const CONFIG_FILES = new Set([
  "package.json",
  "tsconfig.json",
  "pom.xml",
  "go.mod",
  "cargo.toml",
  "pyproject.toml",
  "requirements.txt",
]);
const SOURCE_DIRECTORIES = new Set(["src", "app", "packages"]);
const TEST_DIRECTORIES = new Set(["test", "tests", "__tests__"]);
const ASSET_DIRECTORIES = new Set(["assets", "public", "static"]);
const RULE_FILES = new Set(["AGENTS.md", "CLAUDE.md"]);
const CODE_FILE =
  /\.(ts|tsx|js|jsx|mjs|cjs|java|kt|go|rs|py|rb|php|cs|swift)$/i;

export async function isEmptyProject(root: string): Promise<boolean> {
  const entries = await collectEntries(root);
  return !entries.some(({ path, kind }) =>
    isMeaningfulProjectEntry(path, kind),
  );
}

export async function discoverProjectConventions(
  root: string,
): Promise<ProjectConventionProfile> {
  const entries = await collectEntries(root);
  const files = entries
    .filter((entry) => entry.kind === "file")
    .map(({ path }) => path);
  const directories = entries
    .filter((entry) => entry.kind === "directory")
    .map(({ path }) => path);
  const packageJson = files.includes("package.json")
    ? JSON.parse(await readFile(join(root, "package.json"), "utf8"))
    : null;

  const conventions: ProjectConventionProfile["conventions"] = [];
  if (Array.isArray(packageJson?.workspaces)) {
    conventions.push({
      kind: "workspace",
      value: "package.json#workspaces",
      evidence: ["package.json"],
    });
  }
  if (files.includes("tsconfig.json")) {
    conventions.push({
      kind: "language",
      value: "typescript",
      evidence: ["tsconfig.json"],
    });
  }

  const ruleFiles = await Promise.all(
    files
      .filter((path) => RULE_FILES.has(basename(path)))
      .map(async (path) => ({
        path,
        scope: dirname(path) === "." ? "." : dirname(path),
        sha256: await sha256File(join(root, path)),
      })),
  );

  const profile: ProjectConventionProfile = {
    schemaVersion: "1.2.0",
    projectType: "existing",
    strategy: "discovered",
    directories: {
      source: unique(
        directories.filter((path) => SOURCE_DIRECTORIES.has(basename(path))),
      ),
      test: unique(
        directories.filter((path) => TEST_DIRECTORIES.has(basename(path))),
      ),
      assets: unique(
        directories.filter((path) => ASSET_DIRECTORIES.has(basename(path))),
      ),
      config: unique(files.filter((path) => CONFIG_FILES.has(basename(path)))),
    },
    conventions,
    unknowns: conventions.length === 0 ? ["尚未发现稳定的目录或工具约定"] : [],
    ruleFiles,
    generatedAt: new Date().toISOString(),
    indexHash: sha256Text(
      JSON.stringify({ files, directories, conventions, ruleFiles }),
    ),
  };

  return profile;
}

export function createEmptyProjectProfile(
  root: string,
  strategy: "free-design" | "user-defined",
): ProjectConventionProfile {
  const generatedAt = new Date().toISOString();
  return {
    schemaVersion: "1.2.0",
    projectType: "empty",
    strategy,
    directories: {
      source: [],
      test: [],
      assets: [],
      config: [],
    },
    conventions: [],
    unknowns:
      strategy === "free-design"
        ? ["允许自由设计目录结构，后续生成内容将以首次落盘结构为准"]
        : ["等待用户定义目录结构规范"],
    ruleFiles: [],
    generatedAt,
    indexHash: sha256Text(`${root}:${strategy}:${generatedAt}`),
  };
}

async function collectEntries(
  root: string,
  current = root,
): Promise<Array<{ path: string; kind: "file" | "directory" }>> {
  const items = await readdir(current, { withFileTypes: true });
  const entries: Array<{ path: string; kind: "file" | "directory" }> = [];
  for (const item of items) {
    if (IGNORED_DIRECTORIES.has(item.name)) continue;
    const absolute = join(current, item.name);
    const relativePath = toPosix(relative(root, absolute));
    if (item.isDirectory()) {
      entries.push({ path: relativePath, kind: "directory" });
      entries.push(...(await collectEntries(root, absolute)));
      continue;
    }
    if (item.isFile()) entries.push({ path: relativePath, kind: "file" });
  }
  return entries;
}

function isMeaningfulProjectEntry(
  path: string,
  kind: "file" | "directory",
): boolean {
  const name = basename(path);
  if (kind === "directory") {
    return (
      SOURCE_DIRECTORIES.has(name) ||
      TEST_DIRECTORIES.has(name) ||
      ASSET_DIRECTORIES.has(name)
    );
  }
  if (DOCUMENT_FILE.test(name) || name.endsWith(".md")) return false;
  return CONFIG_FILES.has(name) || CODE_FILE.test(name);
}

async function sha256File(path: string): Promise<string> {
  return sha256Text(await readFile(path, "utf8"));
}

function sha256Text(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

function toPosix(value: string): string {
  return value.split("\\").join("/");
}

function unique(values: string[]): string[] {
  return [...new Set(values)].sort();
}
