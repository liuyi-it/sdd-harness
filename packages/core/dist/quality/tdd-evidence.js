import { isCommandAllowed } from "../security/shell-policy.js";
const PHASES = ["RED", "GREEN", "REFACTOR", "VERIFY"];
function isRecord(value) {
    if (typeof value !== "object" || value === null)
        return false;
    const prototype = Object.getPrototypeOf(value);
    return prototype === Object.prototype || prototype === null;
}
function nonEmptyString(value) {
    return typeof value === "string" && value.trim().length > 0;
}
export function taskEvidenceFailures(task, rawResult) {
    const result = rawResult;
    if (!isRecord(result))
        return [`${task.id} 的执行结果格式无效`];
    const evidence = result.tddEvidence;
    if (!Array.isArray(evidence) || evidence.length === 0)
        return [`${task.id} 缺少 ${task.phase} 阶段证据`];
    const failures = [];
    let observedExpectedFailure = false;
    for (const rawEntry of evidence) {
        if (!isRecord(rawEntry)) {
            failures.push(`${task.id} 的 TDD 证据格式无效`);
            continue;
        }
        const expectedFailurePresent = Object.hasOwn(rawEntry, "expectedFailure");
        if (!PHASES.includes(rawEntry.phase) ||
            rawEntry.phase !== task.phase)
            failures.push(`${task.id} 包含与任务阶段不匹配的证据`);
        if (!nonEmptyString(rawEntry.command))
            failures.push(`${task.id} 的 TDD 证据命令无效`);
        else if (!isCommandAllowed(rawEntry.command))
            failures.push(`${task.id} 的 TDD 证据命令未在允许清单内`);
        if (!nonEmptyString(rawEntry.output))
            failures.push(`${task.id} 的 TDD 证据缺少输出`);
        if (typeof rawEntry.passed !== "boolean")
            failures.push(`${task.id} 的 TDD 证据 passed 无效`);
        if (task.phase === "RED") {
            if (expectedFailurePresent && rawEntry.expectedFailure !== true)
                failures.push(`${task.id} 的 RED expectedFailure 标记无效`);
            if (rawEntry.passed === false && rawEntry.expectedFailure === true)
                observedExpectedFailure = true;
            if (rawEntry.expectedFailure === true && rawEntry.passed !== false)
                failures.push(`${task.id} 未证明观察到预期失败`);
        }
        else if (expectedFailurePresent || rawEntry.passed !== true) {
            failures.push(`${task.id} 的 ${task.phase} 阶段证据未通过`);
        }
    }
    if (task.phase === "RED" && !observedExpectedFailure)
        failures.push(`${task.id} 未证明观察到预期失败`);
    if (task.phase === "VERIFY") {
        const verification = result.verification;
        if (!Array.isArray(verification) || verification.length === 0)
            failures.push(`${task.id} 缺少全部通过的验证证据`);
        else
            for (const rawEntry of verification) {
                if (!isRecord(rawEntry) ||
                    !nonEmptyString(rawEntry.command) ||
                    (nonEmptyString(rawEntry.command) &&
                        !isCommandAllowed(rawEntry.command)) ||
                    !nonEmptyString(rawEntry.output) ||
                    rawEntry.passed !== true)
                    failures.push(`${task.id} 包含无效或未通过的验证证据`);
            }
    }
    if (!Array.isArray(result.verification))
        failures.push(`${task.id} 的验证证据格式无效`);
    else if (task.phase !== "VERIFY")
        for (const rawEntry of result.verification) {
            if (!isRecord(rawEntry) ||
                !nonEmptyString(rawEntry.command) ||
                (nonEmptyString(rawEntry.command) &&
                    !isCommandAllowed(rawEntry.command)) ||
                !nonEmptyString(rawEntry.output) ||
                typeof rawEntry.passed !== "boolean")
                failures.push(`${task.id} 的验证证据格式无效`);
        }
    return [...new Set(failures)];
}
export function tddChainFailures(tasks, results) {
    const failures = [];
    const pairs = new Map();
    for (const task of tasks.filter((task) => task.sliceType !== "REPAIR"))
        for (const requirement of task.requirements)
            for (const scenario of task.scenarios)
                pairs.set(`${requirement}\0${scenario}`, { requirement, scenario });
    for (const { requirement, scenario } of pairs.values()) {
        const chain = tasks.filter((task) => task.sliceType !== "REPAIR" &&
            task.requirements.includes(requirement) &&
            task.scenarios.includes(scenario));
        const phases = chain.map((task) => task.phase);
        if (chain.length !== PHASES.length ||
            phases.some((phase, index) => phase !== PHASES[index])) {
            failures.push(`${requirement}/${scenario} 的 TDD 阶段链缺失、重复或乱序`);
            continue;
        }
        for (let index = 1; index < chain.length; index += 1) {
            const task = chain[index];
            const predecessor = chain[index - 1];
            if (task !== undefined &&
                predecessor !== undefined &&
                !task.dependsOn.includes(predecessor.id))
                failures.push(`${task.id} 缺少直接前驱依赖 ${predecessor.id}`);
        }
        for (const task of chain) {
            const result = results.find((entry) => entry.taskId === task.id);
            if (result !== undefined)
                failures.push(...taskEvidenceFailures(task, result));
        }
        const indexes = chain.map((task) => results.findIndex((result) => result.taskId === task.id));
        if (indexes.every((index) => index >= 0) &&
            indexes.some((index, i) => i > 0 && index <= (indexes[i - 1] ?? -1)))
            failures.push(`${requirement} 的 TDD 执行证据乱序`);
    }
    return failures;
}
//# sourceMappingURL=tdd-evidence.js.map