import { extractRequirementIds } from "../engines/openspec/requirement-ids.js";
import { validateTaskFiles } from "../security/task-scope.js";
import { taskEvidenceFailures, tddChainFailures } from "./tdd-evidence.js";
import { traceabilityFailures } from "./traceability.js";
export function verifyGate(spec, tasks, results, statuses) {
    const failures = [];
    failures.push(...tddChainFailures(tasks, results));
    for (const task of tasks) {
        // verify 阶段同时检查“任务状态已完成”和“任务确实有通过的验证证据”。
        if (statuses[task.id] !== "DONE")
            failures.push(`${task.id} 未完成（DONE）`);
        const result = results.find((entry) => entry.taskId === task.id);
        if (result === undefined)
            failures.push(`${task.id} 缺少执行证据`);
        else
            failures.push(...taskEvidenceFailures(task, result));
    }
    if (typeof spec !== "string")
        failures.push(...traceabilityFailures(spec, tasks, results));
    const requirements = typeof spec === "string"
        ? extractRequirementIds(spec)
        : spec.requirements.map((item) => item.id);
    for (const requirement of requirements) {
        if (!tasks.some((task) => task.requirements.includes(requirement))) {
            failures.push(`${requirement} 未关联到任何任务`);
        }
    }
    if (typeof spec === "string" &&
        !spec.includes("Acceptance Criteria:") &&
        !spec.includes("#### Scenario:"))
        failures.push("缺少验收标准（Acceptance Criteria）");
    return { passed: failures.length === 0, failures };
}
export function reviewGate(tasks, results) {
    const failures = [];
    for (const result of results) {
        // review 更关注结果与任务声明范围是否一致，而不是重新编排执行顺序。
        const task = tasks.find((candidate) => candidate.id === result.taskId);
        if (task === undefined) {
            failures.push(`${result.taskId} 在任务列表中不存在`);
            continue;
        }
        try {
            validateTaskFiles(result.modifiedFiles, task);
        }
        catch (error) {
            failures.push(error instanceof Error ? error.message : String(error));
        }
        if (result.verification.some((entry) => !entry.passed)) {
            failures.push(`${result.taskId} 包含未通过的验证证据`);
        }
    }
    return { passed: failures.length === 0, failures };
}
export function driftFailures(baseline, current, reportedFiles) {
    if (baseline === null || current === null)
        return [];
    if (!baseline.available || !current.available)
        return [];
    const reported = new Set(reportedFiles);
    return current.files
        .filter((file) => baseline.hashes[file] === undefined ||
        baseline.hashes[file] !== current.hashes[file])
        .filter((file) => !reported.has(file))
        .map((file) => `未跟踪到任务结果的变更文件：${file}`);
}
//# sourceMappingURL=quality-gates.js.map