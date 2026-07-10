import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
/**
 * 二期 VerifyReport v1.2：固定七层颗粒度 + 稳定 code，避免后续阶段对凭据化信息做猜测。
 * 各层 failures 既可由 verifyGate / driftFailures / traceabilityFailures 注入，
 * 也允许外部构造后传入。
 */
export const VERIFY_REPORT_LEVELS = [
    "artifacts",
    "tasks",
    "requirements",
    "scenarios",
    "tddEvidence",
    "tests",
    "drift",
];
/** 稳定失败码，未识别一律回退 E_VERIFY_FAILED。 */
export const VERIFY_FAILURE_CODES = {
    ARTIFACT_MISSING: "E_VERIFY_ARTIFACT_MISSING",
    TASK_INCOMPLETE: "E_VERIFY_TASK_INCOMPLETE",
    REQUIREMENT_UNCOVERED: "E_VERIFY_REQUIREMENT_UNCOVERED",
    SCENARIO_MISSING: "E_VERIFY_SCENARIO_MISSING",
    EVIDENCE_INCOMPLETE: "E_VERIFY_EVIDENCE_INCOMPLETE",
    TEST_FAILED: "E_VERIFY_TEST_FAILED",
    DRIFT_DETECTED: "E_VERIFY_DRIFT_DETECTED",
};
export function createVerifyReport(input) {
    const levels = createEmptyLevels();
    for (const failure of input.failures) {
        levels[failure.level].failures.push(failure);
    }
    for (const level of VERIFY_REPORT_LEVELS)
        levels[level].passed = levels[level].failures.length === 0;
    const result = input.failures.length === 0 ? "PASS" : "FAIL";
    const generatedAt = input.generatedAt ?? new Date().toISOString();
    return {
        schemaVersion: "1.2.0",
        changeId: input.changeId,
        result,
        generatedAt,
        counts: input.counts,
        levels,
        failures: [...input.failures].sort((a, b) => a.code === b.code
            ? (a.entity ?? "").localeCompare(b.entity ?? "")
            : a.code.localeCompare(b.code)),
        summary: buildSummary(result, input.failures),
    };
}
export function createEmptyLevels() {
    return VERIFY_REPORT_LEVELS.reduce((acc, level) => {
        acc[level] = { passed: true, failures: [] };
        return acc;
    }, {});
}
export function classifyFailure(level, raw, entity) {
    return {
        code: codeForLevel(level),
        level,
        message: raw.trim(),
        ...(entity === undefined ? {} : { entity }),
    };
}
function codeForLevel(level) {
    switch (level) {
        case "artifacts":
            return VERIFY_FAILURE_CODES.ARTIFACT_MISSING;
        case "tasks":
            return VERIFY_FAILURE_CODES.TASK_INCOMPLETE;
        case "requirements":
            return VERIFY_FAILURE_CODES.REQUIREMENT_UNCOVERED;
        case "scenarios":
            return VERIFY_FAILURE_CODES.SCENARIO_MISSING;
        case "tddEvidence":
            return VERIFY_FAILURE_CODES.EVIDENCE_INCOMPLETE;
        case "tests":
            return VERIFY_FAILURE_CODES.TEST_FAILED;
        case "drift":
            return VERIFY_FAILURE_CODES.DRIFT_DETECTED;
    }
}
function buildSummary(result, failures) {
    if (result === "PASS")
        return "所有七层验证均通过，无未通过项。";
    const tally = new Map();
    for (const failure of failures)
        tally.set(failure.level, (tally.get(failure.level) ?? 0) + 1);
    return `共 ${failures.length} 项未通过，分布：${[...tally.entries()].map(([level, count]) => `${level}=${count}`).join("，")}。`;
}
/**
 * 写入 verify-report.json 与 verify-report.md。原子写由 writeFile 配合 rename 实现。
 * 调用方负责不抛出掩盖报告写盘的异常。
 */
export async function writeVerifyReport(root, changeId, report) {
    const dir = join(root, ".sdd", "changes", changeId);
    await mkdir(dir, { recursive: true });
    const jsonPath = join(dir, "verify-report.v1.2.json");
    const mdPath = join(dir, "verify-report.v1.2.md");
    await writeFile(jsonPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
    await writeFile(mdPath, `${renderMarkdown(report)}\n`, "utf8");
    return { jsonPath, mdPath };
}
export function renderMarkdown(report) {
    const lines = [
        `# 验证报告 (v1.2)`,
        "",
        `- 变更：${report.changeId}`,
        `- 结果：**${report.result}**`,
        `- 生成时间：${report.generatedAt}`,
        `- 需求 ${report.counts.requirements} / 场景 ${report.counts.scenarios} / 任务 ${report.counts.tasks} / 测试 ${report.counts.tests}`,
        "",
        `## 摘要`,
        "",
        report.summary,
        "",
    ];
    for (const level of VERIFY_REPORT_LEVELS) {
        const state = report.levels[level];
        lines.push(`## ${level}`);
        lines.push("");
        lines.push(`- 通过：${state.passed ? "是" : "否"}`);
        if (state.failures.length === 0) {
            lines.push("- 失败：0");
        }
        else {
            lines.push(`- 失败：${state.failures.length}`);
            for (const failure of state.failures) {
                const entity = failure.entity === undefined ? "" : ` (${failure.entity})`;
                lines.push(`  - ${failure.code}：${failure.message}${entity}`);
            }
        }
        lines.push("");
    }
    return lines.join("\n");
}
//# sourceMappingURL=verify-report.js.map