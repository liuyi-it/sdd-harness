import { createReviewIssue, } from "./review-report.js";
/**
 * 复核 review 终止条件：
 * 1. forbiddenFiles 命中；
 * 2. baseline / current Git delta 中存在未被任何任务 reported 的变更；
 * 3. 测试证据中存在未通过的 verification 记录（应被 verify 拦截，但 review 再校验）；
 * 4. 命令证据中出现可疑路径（落到 BLOCKER）。
 */
export function runDeterministicReview(input) {
    const issues = [];
    const currentFiles = new Set();
    if (input.current !== null && input.current.available) {
        for (const file of input.current.files)
            currentFiles.add(file);
    }
    for (const task of input.tasks) {
        issues.push(...forbiddenFileIssues(task, currentFiles));
    }
    if (input.baseline !== null && input.current !== null) {
        issues.push(...unreportedChangeIssues(input.baseline, input.current, input.results));
    }
    for (const result of input.results) {
        issues.push(...testingEvidenceIssues(result));
    }
    issues.push(...specCoverageIssues(input.tasks, input.spec));
    return { issues };
}
function specCoverageIssues(tasks, spec) {
    const requirementIds = new Set(tasks.flatMap((task) => task.requirements));
    const scenarioIds = new Set(tasks.flatMap((task) => task.scenarios));
    const issues = [];
    for (const requirement of spec.requirements) {
        if (!requirementIds.has(requirement.id)) {
            issues.push(createReviewIssue({
                axis: "SPEC",
                category: "BLOCKER",
                severity: "MAJOR",
                message: `需求 ${requirement.id} 没有对应构建任务`,
            }));
        }
        for (const scenario of requirement.scenarios) {
            if (scenarioIds.has(scenario.id))
                continue;
            issues.push(createReviewIssue({
                axis: "SPEC",
                category: "BLOCKER",
                severity: "MAJOR",
                message: `场景 ${scenario.id} 没有对应构建任务`,
            }));
        }
    }
    return issues;
}
function forbiddenFileIssues(task, currentFiles) {
    if (!Array.isArray(task.forbiddenFiles) || task.forbiddenFiles.length === 0) {
        return [];
    }
    const issues = [];
    for (const pattern of task.forbiddenFiles) {
        for (const file of currentFiles) {
            if (!matchesPattern(pattern, file))
                continue;
            issues.push(createReviewIssue({
                category: "FILE_SCOPE",
                severity: "MAJOR",
                task: task.id,
                file,
                message: `任务 ${task.id} 声明禁止修改 ${pattern}，但 current-run 出现 ${file}`,
            }));
        }
    }
    return issues;
}
function matchesPattern(pattern, file) {
    if (pattern === file)
        return true;
    if (pattern.endsWith("/**")) {
        const prefix = pattern.slice(0, -3);
        return file === prefix || file.startsWith(`${prefix}/`);
    }
    if (pattern.startsWith("**/")) {
        return file.endsWith(pattern.slice(3));
    }
    if (pattern.startsWith(".")) {
        return file === pattern.slice(1) || file.endsWith(pattern);
    }
    return false;
}
function unreportedChangeIssues(baseline, current, results) {
    if (!baseline.available || !current.available)
        return [];
    const reported = new Set();
    for (const result of results) {
        if (!Array.isArray(result.modifiedFiles))
            continue;
        for (const file of result.modifiedFiles)
            reported.add(file);
    }
    const issues = [];
    for (const file of current.files) {
        const baselineHash = baseline.hashes[file];
        const currentHash = current.hashes[file];
        const isChanged = baselineHash === undefined || baselineHash !== currentHash;
        if (!isChanged)
            continue;
        if (reported.has(file))
            continue;
        issues.push(createReviewIssue({
            category: "UNRELATED_CHANGE",
            severity: "MAJOR",
            file,
            message: `文件 ${file} 出现在 current-run diff，但未被任何任务的 modifiedFiles 记录`,
        }));
    }
    return issues;
}
function testingEvidenceIssues(result) {
    if (!Array.isArray(result.verification))
        return [];
    const issues = [];
    for (const entry of result.verification) {
        if (entry === null || typeof entry !== "object")
            continue;
        if (entry.passed === false) {
            const severity = "MAJOR";
            issues.push(createReviewIssue({
                category: "TESTING",
                severity,
                task: result.taskId,
                message: `任务 ${result.taskId} 验证命令 ${String(entry.command)} 未通过`,
            }));
        }
        if (typeof entry.command === "string" && entry.command.includes("rm ")) {
            issues.push(createReviewIssue({
                category: "BLOCKER",
                severity: "MAJOR",
                task: result.taskId,
                message: `任务 ${result.taskId} 写入危险命令 ${entry.command}`,
            }));
        }
    }
    return issues;
}
//# sourceMappingURL=deterministic-review.js.map