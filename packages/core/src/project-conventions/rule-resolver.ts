import { createHash } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import { dirname, join } from "node:path";

export type RuleHost = "codex" | "claude-code";

export interface ProjectRuleSnapshot {
  host: RuleHost;
  sources: Array<{
    path: string;
    scope: string;
    sha256: string;
    priority: number;
    content: string;
  }>;
  acknowledgement: "MUST_FOLLOW_PROJECT_RULES";
  hash: string;
}

const RULE_FILES = ["AGENTS.md", "CLAUDE.md"] as const;
const IGNORED_SCOPES = new Set([".git", ".sdd", "node_modules", ".worktrees"]);

export async function resolveProjectRules(
  root: string,
  allowedFiles: string[],
  host: RuleHost = "codex",
): Promise<ProjectRuleSnapshot> {
  const scopes = collectScopes(allowedFiles);
  const sources: ProjectRuleSnapshot["sources"] = [];

  for (const scope of scopes) {
    const directory = scope === "." ? root : join(root, scope);
    let entries: string[] = [];
    try {
      entries = await readdir(directory);
    } catch {
      continue;
    }
    for (const file of RULE_FILES) {
      if (!entries.includes(file)) continue;
      const path = scope === "." ? file : `${scope}/${file}`;
      const content = await readFile(join(directory, file), "utf8");
      sources.push({
        path,
        scope,
        sha256: sha256(content),
        priority: rulePriority(host, file),
        content,
      });
    }
  }

  sources.sort((left, right) => {
    if (left.priority !== right.priority) return left.priority - right.priority;
    const depth = scopeDepth(left.scope) - scopeDepth(right.scope);
    if (depth !== 0) return depth;
    return left.path.localeCompare(right.path);
  });

  return {
    host,
    sources,
    acknowledgement: "MUST_FOLLOW_PROJECT_RULES",
    hash: `sha256:${sha256(
      JSON.stringify(
        sources.map(({ path, scope, sha256, priority }) => ({
          path,
          scope,
          sha256,
          priority,
        })),
      ),
    )}`,
  };
}

function collectScopes(files: string[]): string[] {
  const scopes = new Set<string>(["."]);
  for (const file of files) {
    const normalized = file
      .replace(/\\/g, "/")
      .replace(/\/\*\*$/, "")
      .replace(/^\.\//, "");
    if (normalized === "" || normalized.startsWith("..")) continue;
    const directory =
      normalized.includes(".") && !normalized.endsWith("/")
        ? dirname(normalized)
        : normalized;
    if (directory === ".") continue;
    const segments = directory.split("/");
    if (segments.some((segment) => IGNORED_SCOPES.has(segment))) continue;
    for (let index = 1; index <= segments.length; index += 1) {
      scopes.add(segments.slice(0, index).join("/"));
    }
  }
  return [...scopes].sort(
    (left, right) => scopeDepth(left) - scopeDepth(right),
  );
}

function rulePriority(
  host: RuleHost,
  file: (typeof RULE_FILES)[number],
): number {
  if (host === "codex") return file === "AGENTS.md" ? 0 : 1;
  return file === "CLAUDE.md" ? 0 : 1;
}

function scopeDepth(scope: string): number {
  return scope === "." ? 0 : scope.split("/").length;
}

function sha256(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}
