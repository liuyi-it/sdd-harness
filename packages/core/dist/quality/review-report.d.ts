/**
 * 二期确定性审查：所有 issue 必须是结构化的 ReviewIssue，ID 由稳定字段哈希得出；
 * LLM review 不进入 Core 路径。
 */
export declare const REVIEW_CATEGORIES: readonly ["FILE_SCOPE", "UNRELATED_CHANGE", "SECURITY", "TESTING", "BLOCKER", "SECRET_LEAK"];
export type ReviewCategory = (typeof REVIEW_CATEGORIES)[number];
export declare const REVIEW_SEVERITIES: readonly ["MAJOR", "MINOR", "INFO"];
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
}
export declare function createReviewIssue(input: ReviewIssueInput): ReviewIssue;
export declare function stableId(input: ReviewIssueInput): string;
export interface ReviewReportInput {
    changeId: string;
    issues: ReviewIssue[];
    fixedPoint?: string;
    generatedAt?: string;
}
export declare function createReviewReport(input: ReviewReportInput): ReviewReport;
export declare function isBlocking(issues: readonly ReviewIssue[]): boolean;
export declare function writeReviewReport(root: string, changeId: string, report: ReviewReport): Promise<{
    jsonPath: string;
    mdPath: string;
}>;
export declare function renderReviewMarkdown(report: ReviewReport): string;
//# sourceMappingURL=review-report.d.ts.map