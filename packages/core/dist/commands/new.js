import { mkdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter, artifactInputHash, } from "../artifacts/artifact-writer.js";
import { wrapUntrustedMcpOutput } from "../security/untrusted-content.js";
import { SddError } from "../errors.js";
import { FileLock } from "../state/file-lock.js";
import { StateStore } from "../state/state-store.js";
import { assertRecoverableCommandState, canResumeCommand, normalizeCommandError, persistCommandFailure, previousStablePhase, } from "./recovery.js";
import { timeoutMilliseconds, withTimeout } from "./timeout.js";
export async function runNew(root, rawArgs, engine, codebase, signal) {
    // 兼容 CLI 传入的 "non-interactive" 和 Core 预期的 nonInteractive
    const resolvedRawArgs = { ...rawArgs };
    if (resolvedRawArgs["non-interactive"] !== undefined &&
        resolvedRawArgs.nonInteractive === undefined) {
        resolvedRawArgs.nonInteractive = resolvedRawArgs["non-interactive"];
    }
    const args = parseArgs(resolvedRawArgs);
    const lock = new FileLock(root);
    await lock.acquire("sdd new", args.changeId, lockOptions(rawArgs));
    const store = new StateStore(root);
    let started = false;
    let previousPhase = "INDEX_READY";
    try {
        let state = await store.read();
        const retrying = canResumeCommand(state, "sdd new");
        const storedRequirement = state.currentPhase === "SPEC_READY" && state.currentRunId !== null
            ? (await readFile(join(root, ".sdd", "runs", state.currentRunId, "input.md"), "utf8")).replace(/\n$/, "")
            : undefined;
        const repeating = state.currentPhase === "SPEC_READY" &&
            (args.changeId === undefined ||
                args.changeId === state.currentChangeId) &&
            (args.requirement === undefined ||
                args.requirement === storedRequirement);
        assertRecoverableCommandState(state, "sdd new");
        previousPhase = previousStablePhase(state, "INDEX_READY");
        if (state.currentPhase !== "INDEX_READY" &&
            state.currentPhase !== "CLARIFYING" &&
            state.currentPhase !== "ARCHIVED" &&
            !repeating &&
            !retrying) {
            throw new SddError(state.currentChangeId === null
                ? "E_INVALID_PHASE_COMMAND"
                : "E_ACTIVE_CHANGE_EXISTS", `无法在 ${state.currentPhase} 状态下开启新的变更`, state.suggestedCommand ?? undefined);
        }
        const continuing = state.currentPhase === "CLARIFYING" || repeating || retrying;
        const parentChangeId = state.currentPhase === "ARCHIVED" ? state.currentChangeId : null;
        const changeId = continuing
            ? (state.currentChangeId ?? `change-${Date.now()}`)
            : args.changeId || `change-${Date.now()}`;
        const runId = continuing ? state.currentRunId : `run-${Date.now()}`;
        if (runId === null)
            throw new SddError("E_STATE_CORRUPTED", "处于澄清状态的变更缺少 run id");
        const runDirectory = join(root, ".sdd", "runs", runId);
        const changeDirectory = join(root, ".sdd", "changes", changeId);
        await Promise.all([
            mkdir(runDirectory, { recursive: true }),
            mkdir(changeDirectory, { recursive: true }),
        ]);
        const requirement = continuing
            ? (await readFile(join(runDirectory, "input.md"), "utf8")).replace(/\n$/, "")
            : args.requirement;
        if (requirement === undefined || requirement.trim() === "") {
            throw new SddError("E_MISSING_ARTIFACT", "需求内容不能为空");
        }
        if (!continuing) {
            await new ArtifactWriter().write(join(runDirectory, "input.md"), requirement, {
                changeId,
                requirement,
            });
        }
        state = await store.update((current) => ({
            ...current,
            currentChangeId: changeId,
            currentRunId: runId,
            currentPhase: "NEW_STARTED",
            inProgressPhase: "NEW_STARTED",
            previousPhase,
            lastCommand: "sdd new",
            lastError: null,
            suggestedCommand: "sdd new",
        }));
        started = true;
        const codebaseSummary = await readFile(join(root, ".sdd/index/codebase-summary.md"), "utf8");
        const analysis = engine.analyze(requirement, args.answers);
        const unansweredBlockers = analysis.questions.filter((question) => question.severity === "BLOCKER" && !args.answers?.[question.id]);
        const generationInput = {
            requirement,
            codebaseSummary,
            ...(args.answers === undefined ? {} : { answers: args.answers }),
        };
        const preview = unansweredBlockers.length > 0
            ? clarificationPreview(requirement, codebaseSummary, analysis.questions)
            : await withTimeout(Promise.resolve(engine.generate(generationInput)), timeoutMilliseconds(rawArgs), "sdd new", signal);
        if (parentChangeId !== null) {
            preview.proposal = `${preview.proposal}\n\n## Based On Archived Change\n\n- ${parentChangeId}`;
        }
        const writer = new ArtifactWriter();
        const requirementInputs = {
            requirement,
            answers: args.answers ?? {},
        };
        await Promise.all([
            writer.write(join(changeDirectory, "proposal.md"), preview.proposal, requirementInputs),
            writer.write(join(changeDirectory, "impact.md"), await renderImpactWithMcp(preview.impact, codebase, root, changeId, requirement), { requirement, codebaseSummary }),
            writer.write(join(changeDirectory, "questions.md"), preview.questions, requirementInputs),
        ]);
        if (unansweredBlockers.length > 0) {
            if (args.nonInteractive) {
                await store.update((current) => ({
                    ...current,
                    currentPhase: "FAILED",
                    inProgressPhase: "NEW_STARTED",
                    previousPhase,
                    failedCommand: "sdd new",
                    failedReason: "非交互模式下 BLOCKER 问题必须提供答案",
                    interruptedCommand: null,
                    recoverable: true,
                    lastError: "E_UNRESOLVED_BLOCKER",
                    suggestedCommand: "sdd new",
                }));
                await new AuditLogger(root).write({
                    command: "sdd new",
                    phase: "FAILED",
                    result: "FAIL",
                    changeId,
                    message: "E_UNRESOLVED_BLOCKER",
                });
                throw new SddError("E_UNRESOLVED_BLOCKER", "非交互模式下 BLOCKER 问题必须提供答案", "sdd new");
            }
            const clarifying = await store.update((current) => ({
                ...current,
                currentPhase: "CLARIFYING",
                inProgressPhase: null,
                suggestedCommand: "sdd new",
            }));
            await new AuditLogger(root).write({
                command: "sdd new",
                phase: "CLARIFYING",
                result: "PAUSED",
                changeId,
                message: "等待 BLOCKER 问题的答复",
            });
            return {
                ok: true,
                state: clarifying.currentPhase,
                exitCode: 0,
                changeId,
                next: "sdd new",
            };
        }
        const force = args.force === true;
        let existingSpec;
        if (!force) {
            let sameInput = false;
            try {
                const metaPath = `${join(changeDirectory, "spec.md")}.meta.json`;
                const metadata = JSON.parse(await readFile(metaPath, "utf8"));
                if (metadata.inputHash === artifactInputHash(requirementInputs)) {
                    sameInput = true;
                }
            }
            catch {
                // 文件不存在或无 meta.json
            }
            if (!sameInput) {
                try {
                    existingSpec = {
                        spec: await readFile(join(changeDirectory, "spec.md"), "utf8"),
                        delta: await readFile(join(changeDirectory, "spec.delta.md"), "utf8"),
                        model: JSON.parse(await readFile(join(changeDirectory, "spec.model.json"), "utf8")),
                    };
                }
                catch {
                    // 制品不存在或无法读取
                }
            }
            if (existingSpec !== undefined) {
                generationInput.existingSpec = existingSpec;
            }
        }
        const artifacts = await withTimeout(Promise.resolve(engine.generate(generationInput)), timeoutMilliseconds(rawArgs), "sdd new", signal);
        for (const [name, content] of Object.entries({
            "answers.md": artifacts.answers,
            "assumptions.md": artifacts.assumptions,
        })) {
            await writer.write(join(changeDirectory, name), content, {
                requirement,
                answers: args.answers ?? {},
            });
        }
        await Promise.all([
            writer.write(join(changeDirectory, "spec.md"), artifacts.spec, requirementInputs),
            writer.write(join(changeDirectory, "spec.delta.md"), artifacts.delta, requirementInputs),
            writer.write(join(changeDirectory, "spec.model.json"), JSON.stringify(artifacts.model, null, 2), requirementInputs),
        ]);
        const ready = await store.update((current) => ({
            ...current,
            currentPhase: "SPEC_READY",
            inProgressPhase: null,
            artifacts: {
                ...current.artifacts,
                proposal: "READY",
                impact: "READY",
                spec: "READY",
            },
            failedReason: null,
            interruptedCommand: null,
            recoverable: true,
            suggestedCommand: "sdd design",
            lastError: null,
            failedCommand: null,
        }));
        await new AuditLogger(root).write({
            command: "sdd new",
            phase: ready.currentPhase,
            result: "PASS",
            changeId,
        });
        return {
            ok: true,
            state: ready.currentPhase,
            exitCode: 0,
            changeId,
            next: "sdd design",
        };
    }
    catch (error) {
        const normalized = normalizeCommandError(error, "E_STATE_CORRUPTED", "sdd new");
        if (started && normalized.code !== "E_UNRESOLVED_BLOCKER") {
            await persistCommandFailure(store, normalized, {
                command: "sdd new",
                previousPhase,
                inProgressPhase: "NEW_STARTED",
            });
        }
        throw normalized;
    }
    finally {
        await lock.release();
    }
}
function lockOptions(args) {
    const timeoutMs = timeoutMilliseconds(args);
    return timeoutMs === undefined ? {} : { timeoutMs };
}
function parseArgs(args) {
    if (args === undefined)
        return {};
    return {
        ...(typeof args.requirement === "string"
            ? { requirement: args.requirement }
            : {}),
        ...(typeof args.changeId === "string" ? { changeId: args.changeId } : {}),
        ...(isStringRecord(args.answers) ? { answers: args.answers } : {}),
        ...(typeof args.nonInteractive === "boolean"
            ? { nonInteractive: args.nonInteractive }
            : {}),
        ...(typeof args.force === "boolean" ? { force: args.force } : {}),
    };
}
function isStringRecord(value) {
    return (typeof value === "object" &&
        value !== null &&
        Object.values(value).every((entry) => typeof entry === "string"));
}
function clarificationPreview(requirement, codebaseSummary, questions) {
    return {
        proposal: `# Proposal\n\n## Requested Change\n\n${requirement}`,
        impact: buildImpactPreview(codebaseSummary),
        questions: [
            "# Questions",
            "",
            ...questions.map((question) => `## ${question.id} [${question.severity}]\n\n${question.question}`),
        ].join("\n\n"),
    };
}
/**
 * 调用 MCP intent=impact 查询并附加结构化发现到 impact.md。codebase 不可用或为 fallback
 * 时仍然返回可读结果，仅附加 "degraded=true" 提示。
 */
async function renderImpactWithMcp(baseImpact, codebase, root, changeId, requirement) {
    let normalizedImpact = baseImpact;
    try {
        const codebaseSummary = await readFile(join(root, ".sdd/index/codebase-summary.md"), "utf8");
        normalizedImpact = baseImpact.replace("MCP_OUTPUT_IS_UNTRUSTED_CONTEXT", wrapUntrustedMcpOutput(codebaseSummary, "init/architecture"));
    }
    catch {
        normalizedImpact = baseImpact;
    }
    if (codebase === undefined)
        return normalizedImpact;
    let result;
    try {
        result = await codebase.queryImpact(root, {
            intent: "impact",
            changeId,
            requirement,
        });
    }
    catch {
        return normalizedImpact;
    }
    const findings = ["## MCP Impact Findings", ""];
    findings.push(`- provider: ${result.provider}`, `- degraded: ${result.degraded}`, `- confidence: ${result.confidence}`, `- intent: ${result.intent}`);
    if (result.reason !== undefined)
        findings.push(`- reason: ${result.reason}`);
    findings.push("");
    const payload = result.payload;
    if (payload.files.length > 0) {
        findings.push("### 可能修改的文件", "");
        for (const file of payload.files)
            findings.push(`- ${file}`);
        findings.push("");
    }
    if (payload.symbols.length > 0) {
        findings.push("### 关联符号", "");
        for (const symbol of payload.symbols)
            findings.push(`- ${symbol}`);
        findings.push("");
    }
    if (payload.tests.length > 0) {
        findings.push("### 关联测试", "");
        for (const test of payload.tests)
            findings.push(`- ${test}`);
        findings.push("");
    }
    if (payload.risks.length > 0) {
        findings.push("### 已知风险", "");
        for (const risk of payload.risks)
            findings.push(`- ${risk}`);
        findings.push("");
    }
    return [
        normalizedImpact,
        "",
        wrapUntrustedMcpOutput(findings.join("\n"), "impact"),
    ].join("\n");
}
function buildImpactPreview(codebaseSummary) {
    return `# Impact

## Codebase Context

${wrapUntrustedMcpOutput(codebaseSummary, "init/architecture")}
`;
}
//# sourceMappingURL=new.js.map