import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { parseSpec } from "../engines/openspec/parser.js";
import { validateSpec } from "../engines/openspec/validator.js";
import { SddError } from "../errors.js";
const PHASES = ["RED", "GREEN", "REFACTOR", "VERIFY"];
export async function readAuthoritativeSpec(change, markdown) {
    let raw;
    try {
        raw = await readFile(join(change, "spec.model.json"), "utf8");
    }
    catch (error) {
        if (isMissing(error))
            return {
                document: isLegacySpec(markdown)
                    ? parseLegacySpec(markdown)
                    : parseAndValidate(markdown),
                legacy: true,
            };
        throw error;
    }
    let value;
    try {
        value = JSON.parse(raw);
    }
    catch {
        throw corrupted("spec.model.json 不是有效 JSON");
    }
    if (!isSpecDocument(value))
        throw corrupted("spec.model.json 结构无效");
    assertValid(value, "spec.model.json");
    assertStableIds(value);
    assertRenderSafe(value);
    const parsed = parseAndValidate(markdown);
    if (canonicalJson(value) !== canonicalJson(parsed))
        throw corrupted("spec.model.json 与 spec.md 不一致");
    return { document: value, legacy: false };
}
export function traceabilityFailures(document, tasks, results, requireArtifacts = false) {
    const failures = [];
    const requirements = new Map(document.requirements.map((item) => [item.id, item]));
    const scenarios = new Map(document.requirements.flatMap((requirement) => requirement.scenarios.map((scenario) => [scenario.id, requirement.id])));
    for (const task of tasks) {
        for (const requirement of task.requirements)
            if (!requirements.has(requirement))
                failures.push(`${task.id} 引用了不存在的 Requirement ${requirement}`);
        for (const scenario of task.scenarios) {
            const owner = scenarios.get(scenario);
            if (owner === undefined)
                failures.push(`${task.id} 引用了不存在的 Scenario ${scenario}`);
            else if (!task.requirements.includes(owner))
                failures.push(`${task.id} 的 Scenario ${scenario} 不属于其 Requirement`);
        }
    }
    for (const requirement of document.requirements) {
        if (!tasks.some((task) => task.requirements.includes(requirement.id)))
            failures.push(`${requirement.id} 未关联到任何任务`);
        for (const scenario of requirement.scenarios) {
            const chain = tasks.filter((task) => task.requirements.includes(requirement.id) &&
                task.scenarios.includes(scenario.id));
            for (const phase of PHASES) {
                const phaseTasks = chain.filter((task) => task.phase === phase);
                if (phaseTasks.length === 0)
                    failures.push(`${requirement.id}/${scenario.id} 缺少 ${phase} 任务`);
                else if (phaseTasks.length > 1)
                    failures.push(`${requirement.id}/${scenario.id} 的 ${phase} 任务重复`);
                for (const task of phaseTasks) {
                    const result = results.find((entry) => entry.taskId === task.id);
                    if (result === undefined)
                        continue;
                    if (requireArtifacts &&
                        (!Array.isArray(result.modifiedFiles) ||
                            result.modifiedFiles.length === 0))
                        failures.push(`${task.id} 缺少修改文件`);
                    const evidence = result.tddEvidence?.filter((entry) => entry.phase === phase) ?? [];
                    if (!evidence.some((entry) => entry.command.trim().length > 0))
                        failures.push(`${task.id} 缺少 ${phase} 命令`);
                    if (phase === "RED" &&
                        !evidence.some((entry) => entry.passed === false && entry.expectedFailure === true))
                        failures.push(`${task.id} 缺少 RED 预期失败命令`);
                    if (phase !== "RED" &&
                        !evidence.some((entry) => entry.passed === true))
                        failures.push(`${task.id} 缺少通过的 ${phase} 命令`);
                    if (phase === "VERIFY" &&
                        (!Array.isArray(result.verification) ||
                            !result.verification.some((entry) => entry.command.trim().length > 0 && entry.passed)))
                        failures.push(`${task.id} 缺少最终验证命令`);
                }
            }
        }
    }
    return [...new Set(failures)];
}
export function renderTraceability(document, tasks, results) {
    const lines = ["# 需求追溯", ""];
    for (const requirement of document.requirements) {
        lines.push(`## ${requirement.id}: ${requirement.title}`, "");
        for (const scenario of requirement.scenarios) {
            lines.push(`### ${scenario.id}: ${scenario.title}`, "");
            const chain = tasks.filter((task) => task.requirements.includes(requirement.id) &&
                task.scenarios.includes(scenario.id));
            for (const phase of PHASES) {
                const task = chain.find((candidate) => candidate.phase === phase);
                const result = results.find((entry) => entry.taskId === task.id);
                lines.push(`${phase} 任务：${task.id}`);
                lines.push(`修改文件：${unique(result.modifiedFiles).join("、")}`);
                const commands = unique(result.tddEvidence
                    .filter((entry) => entry.phase === phase)
                    .map((entry) => entry.command));
                lines.push(`${phase} 命令：${commands.join("；")}`);
                if (phase === "VERIFY")
                    lines.push(`最终验证命令：${unique(result.verification.map((entry) => entry.command)).join("；")}`);
                lines.push("");
            }
        }
    }
    const repairTasks = tasks.filter((task) => task.sliceType === "REPAIR");
    if (repairTasks.length > 0) {
        lines.push("## Repair Traceability", "");
        for (const task of repairTasks) {
            const result = results.find((entry) => entry.taskId === task.id);
            lines.push(`### ${task.id}: ${task.title}`, "");
            lines.push(`失败来源：${task.failureContext?.source ?? "unknown"}`, `错误码：${task.failureContext?.errorCode ?? "unknown"}`, `Review Findings：${task.failureContext?.findingIds?.join("、") || "无"}`, `前序 Loop Run：${task.failureContext?.previousRunId ?? "unknown"}`, `Policy：${task.policyRefs?.map((policy) => `${policy.id}@${policy.version} (${policy.digest})`).join("；") || "无"}`, `修改文件：${unique(result?.modifiedFiles ?? []).join("、")}`, `TDD 命令：${unique(result?.tddEvidence.map((entry) => entry.command) ?? []).join("；")}`, `验证命令：${unique(result?.verification.map((entry) => entry.command) ?? []).join("；")}`, "");
        }
    }
    return lines.join("\n");
}
function unique(values) {
    return [...new Set(values)];
}
function parseAndValidate(markdown) {
    let document;
    try {
        document = parseSpec(markdown);
    }
    catch (error) {
        throw corrupted(`spec.md 结构无效：${error instanceof Error ? error.message : String(error)}`);
    }
    assertValid(document, "spec.md");
    return document;
}
function isLegacySpec(markdown) {
    return /^### REQ-\d+:/m.test(markdown);
}
function parseLegacySpec(markdown) {
    const ids = [];
    const transformed = markdown.replace(/^### (REQ-\d+):\s*(.+)$/gm, (_heading, id, title) => {
        ids.push(id);
        return `### Requirement: ${title}`;
    });
    if (ids.length === 0 || new Set(ids).size !== ids.length)
        throw corrupted("legacy spec.md 的 Requirement ID 缺失或重复");
    const titleEnd = transformed.indexOf("\n");
    const withDelta = titleEnd < 0
        ? transformed
        : `${transformed.slice(0, titleEnd)}\n\n## ADDED Requirements${transformed.slice(titleEnd)}`;
    const document = parseAndValidate(withDelta);
    document.requirements.forEach((requirement, index) => {
        requirement.id = ids[index];
        requirement.scenarios.forEach((scenario, scenarioIndex) => {
            scenario.id = `${requirement.id}-SC-${String(scenarioIndex + 1).padStart(3, "0")}`;
        });
    });
    return document;
}
function assertStableIds(document) {
    document.requirements.forEach((requirement, requirementIndex) => {
        const expected = `REQ-${String(requirementIndex + 1).padStart(3, "0")}`;
        if (requirement.id !== expected)
            throw corrupted(`spec.model.json requirements[${requirementIndex}].id 必须为 ${expected}`);
        requirement.scenarios.forEach((scenario, scenarioIndex) => {
            const scenarioExpected = `${expected}-SC-${String(scenarioIndex + 1).padStart(3, "0")}`;
            if (scenario.id !== scenarioExpected)
                throw corrupted(`spec.model.json requirements[${requirementIndex}].scenarios[${scenarioIndex}].id 必须为 ${scenarioExpected}`);
        });
    });
}
function assertRenderSafe(document) {
    const values = [
        document.title,
        ...document.requirements.flatMap((requirement) => [
            requirement.id,
            requirement.title,
            ...requirement.scenarios.flatMap((scenario) => [
                scenario.id,
                scenario.title,
            ]),
        ]),
    ];
    if (values.some((value) => /[\r\n\0]/.test(value) || /^#/m.test(value)))
        throw corrupted("spec.model.json 包含不可安全渲染的字段");
}
function assertValid(document, name) {
    const failures = validateSpec(document);
    if (failures.length > 0)
        throw corrupted(`${name} 校验失败：${failures.map((failure) => failure.message).join("；")}`);
}
function isSpecDocument(value) {
    if (!isRecord(value) ||
        typeof value.title !== "string" ||
        !Array.isArray(value.requirements))
        return false;
    return value.requirements.every((requirement) => isRecord(requirement) &&
        typeof requirement.id === "string" &&
        typeof requirement.title === "string" &&
        typeof requirement.statement === "string" &&
        ["ADDED", "MODIFIED", "REMOVED"].includes(String(requirement.operation)) &&
        Array.isArray(requirement.scenarios) &&
        requirement.scenarios.every((scenario) => isRecord(scenario) &&
            typeof scenario.id === "string" &&
            typeof scenario.title === "string" &&
            [scenario.given, scenario.when, scenario.then].every((steps) => Array.isArray(steps) &&
                steps.every((step) => typeof step === "string"))));
}
function isRecord(value) {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}
function isMissing(error) {
    return error instanceof Error && "code" in error && error.code === "ENOENT";
}
function corrupted(message) {
    return new SddError("E_STATE_CORRUPTED", message);
}
function canonicalJson(value) {
    if (Array.isArray(value))
        return `[${value.map(canonicalJson).join(",")}]`;
    if (isRecord(value))
        return `{${Object.keys(value)
            .sort()
            .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
            .join(",")}}`;
    return JSON.stringify(value);
}
//# sourceMappingURL=traceability.js.map