/**
 * 二期 VerifyReport v1.2：固定七层颗粒度 + 稳定 code，避免后续阶段对凭据化信息做猜测。
 * 各层 failures 既可由 verifyGate / driftFailures / traceabilityFailures 注入，
 * 也允许外部构造后传入。
 */
export declare const VERIFY_REPORT_LEVELS: readonly ["artifacts", "tasks", "requirements", "scenarios", "tddEvidence", "tests", "drift"];
export type VerifyReportLevel = (typeof VERIFY_REPORT_LEVELS)[number];
/** 稳定失败码，未识别一律回退 E_VERIFY_FAILED。 */
export declare const VERIFY_FAILURE_CODES: {
    readonly ARTIFACT_MISSING: "E_VERIFY_ARTIFACT_MISSING";
    readonly TASK_INCOMPLETE: "E_VERIFY_TASK_INCOMPLETE";
    readonly REQUIREMENT_UNCOVERED: "E_VERIFY_REQUIREMENT_UNCOVERED";
    readonly SCENARIO_MISSING: "E_VERIFY_SCENARIO_MISSING";
    readonly EVIDENCE_INCOMPLETE: "E_VERIFY_EVIDENCE_INCOMPLETE";
    readonly TEST_FAILED: "E_VERIFY_TEST_FAILED";
    readonly DRIFT_DETECTED: "E_VERIFY_DRIFT_DETECTED";
};
export type VerifyFailureCode = (typeof VERIFY_FAILURE_CODES)[keyof typeof VERIFY_FAILURE_CODES];
export interface VerifyFailure {
    code: VerifyFailureCode;
    level: VerifyReportLevel;
    message: string;
    entity?: string;
}
export interface VerifyReportLevelState {
    passed: boolean;
    failures: VerifyFailure[];
}
export type VerifyReportLevels = Record<VerifyReportLevel, VerifyReportLevelState>;
export interface VerifyReport {
    schemaVersion: "1.2.0";
    changeId: string;
    result: "PASS" | "FAIL";
    generatedAt: string;
    counts: {
        requirements: number;
        scenarios: number;
        tasks: number;
        tests: number;
    };
    levels: VerifyReportLevels;
    failures: VerifyFailure[];
    summary: string;
}
export interface VerifyReportInput {
    changeId: string;
    counts: {
        requirements: number;
        scenarios: number;
        tasks: number;
        tests: number;
    };
    failures: VerifyFailure[];
    generatedAt?: string;
}
export declare function createVerifyReport(input: VerifyReportInput): VerifyReport;
export declare function createEmptyLevels(): VerifyReportLevels;
export declare function classifyFailure(level: VerifyReportLevel, raw: string, entity?: string): VerifyFailure;
/**
 * 写入 verify-report.json 与 verify-report.md。原子写由 writeFile 配合 rename 实现。
 * 调用方负责不抛出掩盖报告写盘的异常。
 */
export declare function writeVerifyReport(root: string, changeId: string, report: VerifyReport): Promise<{
    jsonPath: string;
    mdPath: string;
}>;
export declare function renderMarkdown(report: VerifyReport): string;
//# sourceMappingURL=verify-report.d.ts.map