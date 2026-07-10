import { SddError } from "../errors.js";
import { isAllowedCommand, parseAllowedCommand, } from "../security/shell-policy.js";
export function normalizeTaskExecutionResult(raw, options) {
    if (isV2Result(raw)) {
        const taskId = raw.taskId;
        if (typeof taskId !== "string" || taskId.length === 0)
            throw new SddError("E_TDD_EVIDENCE_REQUIRED", "v2 执行结果缺少 taskId", "sdd build");
        return {
            ...raw,
            taskId,
            commandEvidence: raw.commandEvidence.map(normalizeStructuredEvidence),
            fileDelta: options.actualFileDelta,
            timestamps: {
                startedAt: options.startedAt,
                endedAt: options.endedAt,
            },
            mode: {
                requested: options.requestedMode,
                actual: options.actualMode,
            },
            ...(options.degradedReason === undefined
                ? {}
                : {
                    notes: [...(raw.notes ?? []), options.degradedReason],
                }),
        };
    }
    const taskId = raw.taskId;
    if (typeof taskId !== "string" || taskId.length === 0)
        throw new SddError("E_TDD_EVIDENCE_REQUIRED", "v1 执行结果缺少 taskId", "sdd build");
    const commandEvidence = [
        ...raw.tddEvidence.map((entry) => normalizeStringEvidence(entry.command, entry.output)),
        ...raw.verification.map((entry) => normalizeStringEvidence(entry.command, entry.output)),
    ];
    return {
        schemaVersion: "1.2.0",
        taskId,
        status: inferStatus(raw),
        summary: summarize(raw),
        commandEvidence,
        fileDelta: options.actualFileDelta,
        timestamps: {
            startedAt: options.startedAt,
            endedAt: options.endedAt,
        },
        mode: {
            requested: options.requestedMode,
            actual: options.actualMode,
        },
        ...(options.degradedReason === undefined
            ? {}
            : {
                notes: [options.degradedReason],
            }),
        legacy: raw,
    };
}
function inferStatus(result) {
    if (result.verification.some((entry) => entry.passed === false))
        return "FAILED";
    return "SUCCEEDED";
}
function summarize(result) {
    const phase = result.tddEvidence[0]?.phase;
    return phase === undefined ? "任务已完成" : `${phase} 阶段已记录执行证据`;
}
function normalizeStringEvidence(command, outputSummary) {
    try {
        const parsed = parseAllowedCommand(command);
        if (!isAllowedCommand(parsed))
            throw new SddError("E_SECURITY_BLOCKED", `命令未在允许清单内：${command}`, "sdd build");
        return {
            ...parsed,
            outputSummary,
        };
    }
    catch (error) {
        if (error instanceof SddError)
            throw error;
        throw new SddError("E_SECURITY_BLOCKED", `命令无法安全解析：${command}`, "sdd build");
    }
}
function normalizeStructuredEvidence(evidence) {
    if (!isAllowedCommand({ command: evidence.command, args: evidence.args }))
        throw new SddError("E_SECURITY_BLOCKED", `命令未在允许清单内：${evidence.command} ${evidence.args.join(" ")}`.trim(), "sdd build");
    return evidence;
}
function isV2Result(value) {
    return (typeof value === "object" &&
        value !== null &&
        "schemaVersion" in value &&
        value.schemaVersion === "1.2.0" &&
        "commandEvidence" in value &&
        "fileDelta" in value);
}
//# sourceMappingURL=task-result-normalizer.js.map