import type { GitSnapshot } from "../git/git-inspector.js";
import { createReviewIssue, type ReviewIssue } from "./review-report.js";

export const DEPENDENCY_SECTIONS = [
  "dependencies",
  "devDependencies",
  "peerDependencies",
  "optionalDependencies",
] as const;

export type DependencySection = (typeof DEPENDENCY_SECTIONS)[number];

export interface DependencyDelta {
  manifest: string;
  section: DependencySection;
  name: string;
  before: string | null;
  after: string | null;
  change: "ADDED" | "REMOVED" | "UPDATED";
}

export interface DependencyDeltaResult {
  dependencies: DependencyDelta[];
  issues: ReviewIssue[];
}

/**
 * 只比较 Git 快照中受支持的 package.json 段落。快照在 build 前后保存文本，
 * 因而不会把执行前已有的工作区改动误算为本次依赖变更。
 */
export function collectDependencyDelta(
  baseline: GitSnapshot,
  current: GitSnapshot,
): DependencyDeltaResult {
  const beforeManifests = baseline.manifests;
  const afterManifests = current.manifests;
  if (beforeManifests === undefined || afterManifests === undefined)
    return { dependencies: [], issues: [] };

  const dependencies: DependencyDelta[] = [];
  const issues: ReviewIssue[] = [];
  const paths = [
    ...new Set([
      ...Object.keys(beforeManifests),
      ...Object.keys(afterManifests),
    ]),
  ].sort();
  for (const manifest of paths) {
    const before = parseManifest(
      beforeManifests[manifest],
      manifest,
      "基线",
      issues,
    );
    const after = parseManifest(
      afterManifests[manifest],
      manifest,
      "当前",
      issues,
    );
    if (before === undefined || after === undefined) continue;
    for (const section of DEPENDENCY_SECTIONS) {
      const beforeSection = dependenciesOf(before, section);
      const afterSection = dependenciesOf(after, section);
      for (const name of [
        ...new Set([
          ...Object.keys(beforeSection),
          ...Object.keys(afterSection),
        ]),
      ].sort()) {
        const beforeVersion = beforeSection[name] ?? null;
        const afterVersion = afterSection[name] ?? null;
        if (beforeVersion === afterVersion) continue;
        dependencies.push({
          manifest,
          section,
          name,
          before: beforeVersion,
          after: afterVersion,
          change:
            beforeVersion === null
              ? "ADDED"
              : afterVersion === null
                ? "REMOVED"
                : "UPDATED",
        });
      }
    }
  }
  return { dependencies, issues };
}

type PackageManifest = Partial<
  Record<DependencySection, Record<string, string>>
>;

function parseManifest(
  content: string | null | undefined,
  manifest: string,
  snapshot: string,
  issues: ReviewIssue[],
): PackageManifest | undefined {
  if (content === null || content === undefined) return {};
  try {
    const parsed: unknown = JSON.parse(content);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed))
      throw new Error("根节点必须是对象");
    for (const section of DEPENDENCY_SECTIONS) {
      const value = (parsed as Record<string, unknown>)[section];
      if (
        value !== undefined &&
        (typeof value !== "object" ||
          value === null ||
          Array.isArray(value) ||
          !Object.values(value).every((entry) => typeof entry === "string"))
      )
        throw new Error(`${section} 必须是 string record`);
    }
    return parsed as PackageManifest;
  } catch (error) {
    issues.push(
      createReviewIssue({
        category: "BLOCKER",
        severity: "MAJOR",
        file: manifest,
        message: `${snapshot} ${manifest} 无法解析依赖：${error instanceof Error ? error.message : "格式无效"}`,
      }),
    );
    return undefined;
  }
}

function dependenciesOf(
  manifest: PackageManifest,
  section: DependencySection,
): Record<string, string> {
  return manifest[section] ?? {};
}
