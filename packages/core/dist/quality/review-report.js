import { createHash } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
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
];
export const REVIEW_SEVERITIES = ["MAJOR", "MINOR", "INFO"];
export function createReviewIssue(input) {
    return {
        id: stableId(input),
        category: input.category,
        severity: input.severity,
        axis: input.axis ?? "STANDARDS",
        message: input.message.trim(),
        ...(input.file === undefined ? {} : { file: input.file }),
        ...(input.task === undefined ? {} : { task: input.task }),
    };
}
export function stableId(input) {
    const seed = JSON.stringify({
        category: input.category,
        axis: input.axis ?? "STANDARDS",
        file: input.file ?? null,
        task: input.task ?? null,
        message: input.message.trim(),
    });
    return "RV-" + createHash("sha256").update(seed).digest("hex").slice(0, 12);
}
const BLOCKING_CATEGORIES = new Set(["BLOCKER", "SECRET_LEAK", "SECURITY", "FILE_SCOPE"]);
const BLOCKING_SEVERITIES = new Set(["MAJOR"]);
export function createReviewReport(input) {
    const issues = dedupe([...input.issues]).sort(byId);
    const severityCounts = countSeverity(issues);
    const categoryCounts = countCategories(issues);
    const blocking = isBlocking(issues);
    const standardsFindings = issues.filter((issue) => issue.axis === "STANDARDS");
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
    };
}
export function isBlocking(issues) {
    return issues.some((issue) => BLOCKING_CATEGORIES.has(issue.category) &&
        BLOCKING_SEVERITIES.has(issue.severity));
}
export async function writeReviewReport(root, changeId, report) {
    const dir = join(root, ".sdd", "changes", changeId);
    await mkdir(dir, { recursive: true });
    const jsonPath = join(dir, "review-report.v2.json");
    const mdPath = join(dir, "review-report.v2.md");
    await writeFile(jsonPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
    await writeFile(mdPath, renderReviewMarkdown(report), "utf8");
    return { jsonPath, mdPath };
}
export function renderReviewMarkdown(report) {
    const lines = [
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
    if (report.issues.length === 0) {
        lines.push("无问题。");
    }
    else {
        for (const issue of report.issues) {
            const file = issue.file === undefined ? "" : ` (${issue.file})`;
            const task = issue.task === undefined ? "" : ` [${issue.task}]`;
            lines.push(`- ${issue.id} ${issue.axis}/${issue.category}/${issue.severity}${file}${task}：${issue.message}`);
        }
    }
    return lines.join("\n") + "\n";
}
function dedupe(issues) {
    const seen = new Map();
    for (const issue of issues)
        seen.set(issue.id, issue);
    return [...seen.values()];
}
function byId(a, b) {
    return a.id.localeCompare(b.id);
}
function countSeverity(issues) {
    const counts = {
        MAJOR: 0,
        MINOR: 0,
        INFO: 0,
    };
    for (const issue of issues)
        counts[issue.severity] += 1;
    return counts;
}
function countCategories(issues) {
    const counts = {};
    for (const issue of issues) {
        counts[issue.category] = (counts[issue.category] ?? 0) + 1;
    }
    return counts;
}
//# sourceMappingURL=review-report.js.map