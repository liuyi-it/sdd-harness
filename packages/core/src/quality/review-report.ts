import { createHash } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";

import type { ChangeComplexityMetrics } from "./change-complexity.js";
import type { DeliberateDebt } from "./deliberate-debt.js";
import type { DependencyDelta } from "./dependency-delta.js";

/**
 * 二期确定性审查：所有 issue 必须是结构化的 ReviewIssue，ID 由稳定字段哈希得出；
 * LLM review 不进入 Core 路径。
 */
export const REVIEW_CATEGORIES = [
  "FILE_SCOPE",
  "UNRELATED_CHANGE",
  "SECURITY",
  "TESTING",
  "BLOCKER",
  "SECRET_LEAK",
  "UNPLANNED_DEPENDENCY",
  "COMPLEXITY",
  "DELIBERATE_DEBT",
] as const;

export type ReviewCategory = (typeof REVIEW_CATEGORIES)[number];

export const REVIEW_SEVERITIES = ["MAJOR", "MINOR", "INFO"] as const;
export type ReviewSeverity = (typeof REVIEW_SEVERITIES)[number];
export type ReviewAxisName = "STANDARDS" | "SPEC";

export interface ReviewIssue {
  id: string;
  category: ReviewCategory;
  severity: ReviewSeverity;
  axis: ReviewAxisName;
  file?: string;
  task?: string;
  message: string;
  deterministic?: boolean;
}

export interface ReviewReport {
  schemaVersion: "2.0.0";
  changeId: string;
  fixedPoint: string;
  result: "PASS" | "BLOCK";
  generatedAt: string;
  severityCounts: Record<ReviewSeverity, number>;
  categoryCounts: Partial<Record<ReviewCategory, number>>;
  issues: ReviewIssue[];
  message: string;
  standards: ReviewAxis;
  spec: ReviewAxis;
  summary: {
    standardsFindingCount: number;
    specFindingCount: number;
  };
  minimality?: {
    metrics: ChangeComplexityMetrics;
    dependencyDelta: DependencyDelta[];
    deliberateDebts: DeliberateDebt[];
    estimatedRemovableLines?: number;
  };
}

export interface ReviewAxis {
  status: "PASSED" | "FAILED" | "SKIPPED";
  findings: ReviewIssue[];
}

export interface ReviewIssueInput {
  category: ReviewCategory;
  severity: ReviewSeverity;
  message: string;
  file?: string;
  task?: string;
  axis?: ReviewAxisName;
  deterministic?: boolean;
}

export function createReviewIssue(input: ReviewIssueInput): ReviewIssue {
  return {
    id: stableId(input),
    category: input.category,
    severity: input.severity,
    axis: input.axis ?? "STANDARDS",
    message: input.message.trim(),
    deterministic: input.deterministic ?? true,
    ...(input.file === undefined ? {} : { file: input.file }),
    ...(input.task === undefined ? {} : { task: input.task }),
  };
}

export function stableId(input: ReviewIssueInput): string {
  const seed = JSON.stringify({
    category: input.category,
    axis: input.axis ?? "STANDARDS",
    file: input.file ?? null,
    task: input.task ?? null,
    message: input.message.trim(),
    deterministic: input.deterministic ?? true,
  });
  return "RV-" + createHash("sha256").update(seed).digest("hex").slice(0, 12);
}

export interface ReviewReportInput {
  changeId: string;
  issues: ReviewIssue[];
  fixedPoint?: string;
  generatedAt?: string;
  minimality?: ReviewReport["minimality"];
}

const BLOCKING_CATEGORIES: ReadonlySet<ReviewCategory> =
  new Set<ReviewCategory>([
    "BLOCKER",
    "SECRET_LEAK",
    "SECURITY",
    "FILE_SCOPE",
    "UNPLANNED_DEPENDENCY",
  ]);

const BLOCKING_SEVERITIES: ReadonlySet<ReviewSeverity> =
  new Set<ReviewSeverity>(["MAJOR"]);

export function createReviewReport(input: ReviewReportInput): ReviewReport {
  const issues = dedupe([...input.issues]).sort(byId);
  const severityCounts = countSeverity(issues);
  const categoryCounts = countCategories(issues);
  const blocking = isBlocking(issues);
  const standardsFindings = issues.filter(
    (issue) => issue.axis === "STANDARDS",
  );
  const specFindings = issues.filter((issue) => issue.axis === "SPEC");
  const generatedAt = input.generatedAt ?? new Date().toISOString();
  return {
    schemaVersion: "2.0.0",
    changeId: input.changeId,
    fixedPoint: input.fixedPoint ?? "unknown",
    result: blocking ? "BLOCK" : "PASS",
    generatedAt,
    severityCounts,
    categoryCounts,
    issues,
    standards: {
      status: isBlocking(standardsFindings) ? "FAILED" : "PASSED",
      findings: standardsFindings,
    },
    spec: {
      status: isBlocking(specFindings) ? "FAILED" : "PASSED",
      findings: specFindings,
    },
    message: blocking
      ? `审查阻断：${issues.length} 个问题含禁止类别或严重级别`
      : `审查通过：${issues.length} 个问题均不阻断归档`,
    summary: {
      standardsFindingCount: standardsFindings.length,
      specFindingCount: specFindings.length,
    },
    ...(input.minimality === undefined ? {} : { minimality: input.minimality }),
  };
}

export function isBlocking(issues: readonly ReviewIssue[]): boolean {
  return issues.some(
    (issue) =>
      BLOCKING_CATEGORIES.has(issue.category) &&
      BLOCKING_SEVERITIES.has(issue.severity),
  );
}

export async function writeReviewReport(
  root: string,
  changeId: string,
  report: ReviewReport,
): Promise<{ jsonPath: string; mdPath: string }> {
  const dir = join(root, ".sdd", "changes", changeId);
  await mkdir(dir, { recursive: true });
  const jsonPath = join(dir, "review-report.v2.json");
  const mdPath = join(dir, "review-report.v2.md");
  await writeFile(jsonPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
  await writeFile(mdPath, renderReviewMarkdown(report), "utf8");
  return { jsonPath, mdPath };
}

export function renderReviewMarkdown(report: ReviewReport): string {
  const lines: string[] = [
    "# 审查报告 (v2)",
    "",
    `- 变更：${report.changeId}`,
    `- 结果：**${report.result}**`,
    `- Fixed Point：${report.fixedPoint}`,
    `- 生成时间：${report.generatedAt}`,
    `- 严重度计数：${Object.entries(report.severityCounts)
      .map(([k, v]) => `${k}=${v}`)
      .join(", ")}`,
    "",
    "## 摘要",
    "",
    report.message,
    `- Standards findings: ${report.summary.standardsFindingCount}`,
    `- Spec findings: ${report.summary.specFindingCount}`,
    "",
    "## Standards Axis",
    "",
    report.standards.status,
    "",
    "## Spec Axis",
    "",
    report.spec.status,
    "",
  ];
  if (report.minimality !== undefined) {
    const { metrics, dependencyDelta, deliberateDebts } = report.minimality;
    lines.push(
      "## 实现简洁性",
      "",
      `- 文件：新增 ${metrics.filesAdded}，修改 ${metrics.filesModified}，删除 ${metrics.filesDeleted}`,
      `- 行数：新增 ${metrics.linesAdded ?? "未记录"}，删除 ${metrics.linesDeleted ?? "未记录"}`,
      `- 依赖：新增 ${metrics.dependenciesAdded}，删除 ${metrics.dependenciesRemoved}`,
      `- 有意债务：${metrics.deliberateDebtCount}`,
      "",
      dependencyDelta.length === 0 ? "依赖变化：未记录。" : "依赖变化：",
      ...dependencyDelta.map(
        (item) =>
          `- ${item.change} ${item.manifest} ${item.section}.${item.name}：${item.before ?? "(none)"} → ${item.after ?? "(none)"}`,
      ),
      "",
      deliberateDebts.length === 0
        ? "有意接受的工程限制：未记录。"
        : "有意接受的工程限制：",
      ...deliberateDebts.map(
        (item) =>
          `- ${item.file}:${item.line}：${item.ceiling}；触发条件=${item.trigger}；升级=${item.upgrade}`,
      ),
      "",
    );
  }
  if (report.issues.length === 0) {
    lines.push("无问题。");
  } else {
    for (const issue of report.issues) {
      const file = issue.file === undefined ? "" : ` (${issue.file})`;
      const task = issue.task === undefined ? "" : ` [${issue.task}]`;
      lines.push(
        `- ${issue.id} ${issue.axis}/${issue.category}/${issue.severity}${file}${task}：${issue.message}`,
      );
    }
  }
  return lines.join("\n") + "\n";
}

function dedupe(issues: ReviewIssue[]): ReviewIssue[] {
  const seen = new Map<string, ReviewIssue>();
  for (const issue of issues) seen.set(issue.id, issue);
  return [...seen.values()];
}

function byId(a: ReviewIssue, b: ReviewIssue): number {
  return a.id.localeCompare(b.id);
}

function countSeverity(
  issues: readonly ReviewIssue[],
): Record<ReviewSeverity, number> {
  const counts: Record<ReviewSeverity, number> = {
    MAJOR: 0,
    MINOR: 0,
    INFO: 0,
  };
  for (const issue of issues) counts[issue.severity] += 1;
  return counts;
}

function countCategories(
  issues: readonly ReviewIssue[],
): Partial<Record<ReviewCategory, number>> {
  const counts: Partial<Record<ReviewCategory, number>> = {};
  for (const issue of issues) {
    counts[issue.category] = (counts[issue.category] ?? 0) + 1;
  }
  return counts;
}
