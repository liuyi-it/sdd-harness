import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join } from "node:path";
import { AuditLogger } from "../audit/audit-logger.js";
import { artifactInputHash } from "../artifacts/artifact-writer.js";
import { readCompactPlan, readCompactSpec, } from "../artifacts/change-artifacts.js";
import { readContextPackMetadata, renderContextPack, stripManagedSections, verifyContextPackDigest, } from "../build/context-pack.js";
import { normalizeTaskExecutionResult, } from "../build/task-result-normalizer.js";
import { assertRecoverableCommandState, canResumeCommand } from "./recovery.js";
import { SddError } from "../errors.js";
import { GitInspector } from "../git/git-inspector.js";
import { isCommandAllowed } from "../security/shell-policy.js";
import { buildTaskConstraints } from "../security/untrusted-content.js";
import { validateTaskFiles } from "../security/task-scope.js";
import { FileLock } from "../state/file-lock.js";
import { scopePatternsOverlap } from "../security/scope-overlap.js";
import { taskEvidenceFailures, tddChainFailures, } from "../quality/tdd-evidence.js";
import { resolveProjectRules, } from "../project-conventions/rule-resolver.js";
import { StateStore } from "../state/state-store.js";
import { parseTaskResults, parseTasks } from "../quality/quality-schema.js";
import { assertChangeWritable, requireActiveChangeId } from "./change-id.js";
import { resolvePolicyBundle } from "@sdd-harness/agent-policies";
export async function runBuild(root, executor, signal, rawArgs) {
    const subcommand = rawArgs?.subcommand;
    // build next：返回下一个待执行任务的 AGENT_TASK_EXECUTION
    if (subcommand === "next") {
        return buildNextTask(root, rawArgs);
    }
    // build complete：验收 Agent 提交的 TaskExecutionResult
    if (subcommand === "complete") {
        return buildCompleteTask(root, rawArgs);
    }
    // 无子命令：完整 build 流程（传统模式）
    const lock = new FileLock(root);
    await lock.acquire("sdd build", undefined, lockOptions(rawArgs));
    const store = new StateStore(root);
    const activeTasks = new Set();
    try {
        const state = await store.read();
        const retrying = canResumeCommand(state, "sdd build");
        assertRecoverableCommandState(state, "sdd build");
        if (state.currentPhase !== "PLAN_READY" && !retrying) {
            throw new SddError("E_INVALID_PHASE_COMMAND", `无法在 ${state.currentPhase} 状态下执行 build`, state.suggestedCommand ?? undefined);
        }
        const changeId = requireActiveChangeId(state.currentChangeId, rawArgs);
        await assertChangeWritable(root, changeId);
        const businessRoot = resolveBusinessRoot(root, state);
        const change = join(root, ".sdd", "changes", changeId);
        const tasks = parseTasks(JSON.stringify((await readCompactPlan(change)).tasks));
        const previousResults = await readResults(join(change, "task-results.json"), { ignoreInvalid: state.currentPhase === "FAILED" });
        const trustedPreviousResults = previousResults.filter((result) => {
            const task = tasks.find((candidate) => candidate.id === result.taskId);
            return (task !== undefined && taskEvidenceFailures(task, result).length === 0);
        });
        const results = trustedPreviousResults.filter((result) => state.tasks[result.taskId] === "DONE");
        await store.update((current) => ({
            ...current,
            currentPhase: "BUILDING",
            inProgressPhase: "BUILDING",
            previousPhase: "PLAN_READY",
            lastCommand: "sdd build",
            lastError: null,
        }));
        const completed = new Set(results.map((result) => result.taskId));
        const remaining = tasks.filter((task) => !completed.has(task.id));
        const git = new GitInspector(businessRoot);
        let gitBefore = await git.snapshot();
        const warnings = gitBefore.available && gitBefore.files.length > 0
            ? [
                `检测到执行前已有未提交修改：${gitBefore.files.slice(0, 5).join(", ")}${gitBefore.files.length > 5 ? ` 等 ${gitBefore.files.length} 个文件` : ""}`,
            ]
            : [];
        await writeFile(join(change, "git-baseline.json"), `${JSON.stringify(gitBefore, null, 2)}\n`, "utf8");
        while (remaining.length > 0) {
            // 只有依赖已经全部完成的任务才允许进入当前批次。
            const readyTasks = remaining.filter((task) => task.dependsOn.every((dependency) => completed.has(dependency)));
            if (readyTasks.length === 0)
                throw new SddError("E_STATE_CORRUPTED", "任务依赖图存在环或不完整");
            const batch = selectParallelBatch(readyTasks);
            for (const task of batch) {
                remaining.splice(remaining.findIndex((candidate) => candidate.id === task.id), 1);
                activeTasks.add(task.id);
            }
            await store.update((current) => ({
                ...current,
                tasks: {
                    ...current.tasks,
                    ...Object.fromEntries(batch.map((task) => [task.id, "BUILDING"])),
                },
            }));
            if (signal?.aborted === true)
                throw interruptionError();
            const executions = await Promise.all(batch.map(async (task) => {
                const startedAt = new Date().toISOString();
                await ensureFreshContextPacks(root, changeId, [task], readHost(rawArgs));
                const contextPack = await readFile(join(root, ".sdd", "context-packs", changeId, `${task.id}.md`), "utf8");
                const projectRules = await resolveProjectRules(root, [...task.allowedFiles, ...task.expectedNewFiles], readHost(rawArgs));
                const result = await executeWithLimits(executor, {
                    schemaVersion: "1.2.0",
                    root: businessRoot,
                    changeId,
                    runId: state.currentRunId ?? "unknown-run",
                    task,
                    contextPack,
                    gitBaseline: gitBefore.available ? gitBefore : null,
                    constraints: buildTaskConstraints({
                        allowedFiles: task.allowedFiles,
                        expectedNewFiles: task.expectedNewFiles,
                        forbiddenFiles: task.forbiddenFiles,
                        allowedCommands: task.verification,
                        maxExecutionMs: timeoutMilliseconds(rawArgs) ?? 0,
                    }),
                    mode: "main-agent",
                    projectRules,
                    ...(signal === undefined ? {} : { signal }),
                }, signal, timeoutMilliseconds(rawArgs));
                return { task, result, startedAt, endedAt: new Date().toISOString() };
            }));
            const invalid = [];
            const shaped = executions.filter(({ task, result }) => {
                try {
                    assertExecutionResultShape(task.id, result);
                    return true;
                }
                catch (error) {
                    invalid.push(error);
                    return false;
                }
            });
            const gitAfter = await git.snapshot();
            const actualDelta = git.delta(gitBefore, gitAfter);
            const actualFilesByTask = adjudicateActualFiles(batch, actualDelta);
            const accepted = [];
            for (const { task, result, startedAt, endedAt } of shaped) {
                try {
                    const legacySource = toLegacyResult(task.id, result);
                    const legacy = validateExecution(task, result, actualFilesByTask.get(task.id)?.length
                        ? (actualFilesByTask.get(task.id) ?? [])
                        : legacySource.modifiedFiles);
                    accepted.push({
                        legacy,
                        artifact: normalizeTaskExecutionResult(attachTaskId(task.id, result, legacy), {
                            actualFileDelta: {
                                added: [],
                                modified: legacy.modifiedFiles,
                                deleted: [],
                            },
                            startedAt,
                            endedAt,
                            requestedMode: "subagent",
                            actualMode: "main-agent",
                            degradedReason: "当前宿主未接入 subagent，已降级为 main-agent 执行",
                        }),
                    });
                }
                catch (error) {
                    invalid.push(error);
                }
            }
            for (const taskResult of accepted) {
                results.push(taskResult.legacy);
                completed.add(taskResult.legacy.taskId);
                activeTasks.delete(taskResult.legacy.taskId);
                const resultDirectory = join(root, ".sdd", "runs", state.currentRunId ?? "unknown-run", "tasks");
                await mkdir(resultDirectory, { recursive: true });
                await writeFile(join(resultDirectory, `${taskResult.legacy.taskId}.result.json`), `${JSON.stringify(taskResult.artifact, null, 2)}\n`, "utf8");
            }
            await store.update((current) => ({
                ...current,
                tasks: {
                    ...current.tasks,
                    ...Object.fromEntries(accepted.map((result) => [result.legacy.taskId, "DONE"])),
                },
            }));
            await writeFile(join(change, "task-results.json"), `${JSON.stringify(results, null, 2)}\n`, "utf8");
            gitBefore = gitAfter;
            if (invalid[0] !== undefined)
                throw invalid[0];
        }
        const chainFailures = tddChainFailures(tasks, results);
        if (chainFailures.length > 0)
            throw new SddError("E_TDD_EVIDENCE_REQUIRED", chainFailures.join("；"), "sdd build");
        const ready = await store.update((current) => ({
            ...current,
            currentPhase: "BUILD_READY",
            inProgressPhase: null,
            failedCommand: null,
            lastError: null,
            suggestedCommand: "sdd verify",
        }));
        await new AuditLogger(root).write({
            command: "sdd build",
            phase: ready.currentPhase,
            result: "PASS",
            changeId,
        });
        return {
            ok: true,
            state: ready.currentPhase,
            exitCode: 0,
            changeId,
            next: "sdd verify",
            ...(warnings.length === 0 ? {} : { warnings }),
        };
    }
    catch (error) {
        if (error instanceof SddError) {
            await store.update((current) => ({
                ...current,
                currentPhase: error.exitCode === 130 ? "PAUSED" : "FAILED",
                previousPhase: "PLAN_READY",
                inProgressPhase: "BUILDING",
                failedCommand: error.exitCode === 130 ? null : "sdd build",
                failedReason: error.exitCode === 130 ? null : error.message,
                interruptedCommand: error.exitCode === 130 ? "sdd build" : null,
                lastError: error.code,
                suggestedCommand: "sdd build",
                ...(activeTasks.size === 0
                    ? {}
                    : {
                        tasks: {
                            ...current.tasks,
                            ...Object.fromEntries([...activeTasks].map((taskId) => [taskId, "FAILED"])),
                        },
                    }),
            }));
            await new AuditLogger(root).write({
                command: "sdd build",
                phase: error.exitCode === 130 ? "PAUSED" : "FAILED",
                result: "FAIL",
                message: error.code,
            });
        }
        throw error;
    }
    finally {
        await lock.release();
    }
}
function resolveBusinessRoot(controlRoot, state) {
    const worktreePath = state.workspace?.worktreePath;
    if (typeof worktreePath !== "string" || worktreePath.length === 0) {
        return controlRoot;
    }
    return isAbsolute(worktreePath)
        ? worktreePath
        : join(controlRoot, worktreePath);
}
function assertExecutionResultShape(taskId, value) {
    const record = value;
    if (record?.schemaVersion === "1.2.0" && isValidV2Envelope(record)) {
        return;
    }
    if (typeof value !== "object" ||
        value === null ||
        (Object.getPrototypeOf(value) !== Object.prototype &&
            Object.getPrototypeOf(value) !== null) ||
        !Array.isArray(record?.modifiedFiles) ||
        !record.modifiedFiles.every((file) => typeof file === "string") ||
        !Array.isArray(record.tddEvidence) ||
        !Array.isArray(record.verification))
        throw new SddError("E_TDD_EVIDENCE_REQUIRED", `任务 ${taskId} 的执行结果结构无效`, "sdd build");
}
function isValidV2Envelope(record) {
    const fileDelta = record.fileDelta;
    const timestamps = record.timestamps;
    const mode = record.mode;
    return (VALID_TASK_STATUSES.includes(record.status) &&
        typeof record.summary === "string" &&
        record.summary.trim().length > 0 &&
        Array.isArray(record.commandEvidence) &&
        record.commandEvidence.every(isValidCommandEvidence) &&
        isStringArrayRecord(fileDelta, ["added", "modified", "deleted"]) &&
        isStringRecord(timestamps, ["startedAt", "endedAt"]) &&
        (mode === undefined ||
            (isRecord(mode) &&
                ["subagent", "main-agent"].includes(String(mode.requested)) &&
                ["subagent", "main-agent"].includes(String(mode.actual)))) &&
        (record.notes === undefined || isStringArray(record.notes)));
}
function isValidCommandEvidence(value) {
    if (!isRecord(value))
        return false;
    return (typeof value.command === "string" &&
        value.command.trim().length > 0 &&
        isStringArray(value.args) &&
        (value.exitCode === undefined || typeof value.exitCode === "number") &&
        typeof value.outputSummary === "string");
}
function isStringArrayRecord(value, keys) {
    return isRecord(value) && keys.every((key) => isStringArray(value[key]));
}
function isStringRecord(value, keys) {
    return isRecord(value) && keys.every((key) => typeof value[key] === "string");
}
function isStringArray(value) {
    return (Array.isArray(value) && value.every((item) => typeof item === "string"));
}
function isRecord(value) {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}
function validateExecution(task, result, actualModifiedFiles) {
    const legacy = toLegacyResult(task.id, result);
    const modifiedFiles = [...new Set(actualModifiedFiles)];
    validateTaskFiles(modifiedFiles, task);
    const evidenceFailures = taskEvidenceFailures(task, legacy);
    if (evidenceFailures.length > 0) {
        const blockedCommand = legacy.tddEvidence.find((entry) => typeof entry === "object" &&
            entry !== null &&
            typeof entry.command === "string" &&
            !isCommandAllowed(entry.command))?.command;
        if (blockedCommand !== undefined)
            throw new SddError("E_SECURITY_BLOCKED", `TDD 证据命令未在允许清单内：${blockedCommand}`);
        throw new SddError("E_TDD_EVIDENCE_REQUIRED", evidenceFailures.join("；"), "sdd build");
    }
    for (const evidence of legacy.verification)
        if (!isCommandAllowed(evidence.command))
            throw new SddError("E_SECURITY_BLOCKED", `验证命令未在允许清单内：${evidence.command}`);
    if (legacy.verification.some((evidence) => !evidence.passed))
        throw new SddError("E_VERIFY_FAILED", `任务 ${task.id} 验证失败`, "sdd build");
    return {
        taskId: task.id,
        schemaVersion: "1.0.0",
        status: "DONE",
        ...legacy,
        modifiedFiles,
    };
}
function lockOptions(args) {
    const timeoutMs = timeoutMilliseconds(args);
    return timeoutMs === undefined ? {} : { timeoutMs };
}
async function ensureFreshContextPacks(root, changeId, tasks, host) {
    const change = join(root, ".sdd", "changes", changeId);
    const [spec, design, compactSpec, compactPlan, codebaseSummary] = await Promise.all([
        readFile(join(root, ".sdd", "changes", changeId, "spec.md"), "utf8"),
        readFile(join(root, ".sdd", "changes", changeId, "design.md"), "utf8"),
        readCompactSpec(change),
        readCompactPlan(change),
        readFile(join(root, ".sdd", "index", "codebase-summary.md"), "utf8"),
    ]);
    const impact = compactSpec.impact;
    const tasksMarkdown = compactPlan.tasksMarkdown;
    const tasksJson = JSON.stringify(compactPlan.tasks, null, 2);
    const expectedCodebaseHash = artifactInputHash(codebaseSummary);
    const expectedSourceHash = artifactInputHash({
        spec,
        design,
        impact,
        tasksMarkdown,
        tasksJson,
    });
    const projectConventionsHash = await readProjectConventionsHash(root);
    for (const task of tasks) {
        const path = join(root, ".sdd", "context-packs", changeId, `${task.id}.md`);
        let contextPack;
        let metadata;
        try {
            contextPack = await readFile(path, "utf8");
            metadata = readContextPackMetadata(contextPack);
        }
        catch {
            contextPack = undefined;
            metadata = undefined;
        }
        const rules = await resolveProjectRules(root, [...task.allowedFiles, ...task.expectedNewFiles], host);
        const digestValid = contextPack !== undefined && verifyContextPackDigest(contextPack);
        if (metadata === undefined ||
            contextPack === undefined ||
            !digestValid ||
            metadata.codebaseIndexHash !== expectedCodebaseHash ||
            metadata.sourceArtifactHash !== expectedSourceHash ||
            metadata.projectRulesHash !== rules.hash ||
            metadata.projectConventionsHash !== projectConventionsHash) {
            await mkdir(dirname(path), { recursive: true });
            await writeFile(path, renderContextPack({
                body: contextPack === undefined || !digestValid
                    ? renderFallbackTaskBody(task)
                    : stripManagedSections(contextPack),
                rules,
                codebaseSummary,
                spec,
                design,
                impact,
                tasksMarkdown,
                tasksJson,
                projectConventionsHash,
                references: contextPackReferences(changeId),
                task: {
                    taskId: task.id,
                    objective: task.title,
                    userVisibleOutcome: task.userVisibleOutcome ?? task.title,
                    requiredFiles: task.allowedFiles,
                    allowedFiles: task.allowedFiles,
                    forbiddenFiles: task.forbiddenFiles,
                    verification: task.verification,
                },
                policyBundle: resolvePolicyBundle({
                    command: "build",
                    phase: "PLAN_READY",
                    ...(task.failureContext === undefined
                        ? {}
                        : { failureCode: task.failureContext.errorCode }),
                }),
            }), "utf8");
        }
    }
}
function contextPackReferences(changeId) {
    return {
        spec: `.sdd/changes/${changeId}/spec.md`,
        design: `.sdd/changes/${changeId}/design.md`,
        plan: `.sdd/changes/${changeId}/plan.json`,
        impact: `.sdd/changes/${changeId}/spec.json`,
        codebase: ".sdd/index/codebase-summary.md",
    };
}
function renderFallbackTaskBody(task) {
    return [
        `# Task: ${task.id}`,
        "",
        `Phase: ${task.phase}`,
        "",
        "## Description",
        "",
        task.title,
    ].join("\n");
}
function selectParallelBatch(tasks) {
    const batch = [];
    for (const task of tasks) {
        if (batch.every((selected) => !taskScopesOverlap(task, selected)))
            batch.push(task);
    }
    return batch.length === 0 && tasks[0] !== undefined ? [tasks[0]] : batch;
}
function taskScopesOverlap(left, right) {
    const leftPatterns = [...left.allowedFiles, ...left.expectedNewFiles];
    const rightPatterns = [...right.allowedFiles, ...right.expectedNewFiles];
    return scopePatternsOverlap(leftPatterns, rightPatterns);
}
function adjudicateActualFiles(tasks, actualFiles) {
    const allocations = new Map(tasks.map((task) => [task.id, []]));
    for (const file of actualFiles) {
        const owners = tasks.filter((task) => fileFitsTask(file, task));
        if (owners.length === 0) {
            throw new SddError("E_SECURITY_BLOCKED", `Git delta 中存在超出任务范围的文件：${file}`);
        }
        if (owners.length > 1) {
            throw new SddError("E_PARALLEL_FILE_CONFLICT", `该文件可能归属于多个并行任务：${file}`);
        }
        allocations.get(owners[0].id)?.push(file);
    }
    return allocations;
}
function fileFitsTask(file, task) {
    try {
        validateTaskFiles([file], task);
        return true;
    }
    catch {
        return false;
    }
}
function classifyFileDelta(before, after, files) {
    const added = [];
    const modified = [];
    const deleted = [];
    for (const file of files) {
        if (!after.files.includes(file) || after.hashes[file] === "deleted")
            deleted.push(file);
        else if (!before.tracked.includes(file))
            added.push(file);
        else
            modified.push(file);
    }
    return { added, modified, deleted };
}
async function readResults(path, options = {}) {
    try {
        const parsed = parseTaskResults(await readFile(path, "utf8"));
        return parsed.map((result) => {
            if (result.status !== "DONE" && result.status !== "SUCCEEDED")
                throw new SddError("E_STATE_CORRUPTED", "task-results.json 包含未成功的任务结果");
            return result;
        });
    }
    catch (error) {
        if (isMissingFile(error))
            return [];
        if (options.ignoreInvalid && error instanceof SddError)
            return [];
        if (error instanceof SddError)
            throw error;
        throw new SddError("E_STATE_CORRUPTED", "task-results.json 无法解析");
    }
}
function isMissingFile(error) {
    return (typeof error === "object" &&
        error !== null &&
        "code" in error &&
        error.code === "ENOENT");
}
function interruptionError() {
    return new SddError("E_INTERRUPTED", "build 已被中断", "sdd build");
}
function timeoutMilliseconds(args) {
    const seconds = args?.timeout;
    return typeof seconds === "number" && seconds > 0
        ? seconds * 1_000
        : undefined;
}
async function executeWithLimits(executor, request, signal, timeout) {
    const promises = [
        executor.execute(request),
    ];
    if (timeout !== undefined) {
        promises.push(new Promise((_, reject) => {
            setTimeout(() => reject(new SddError("E_TIMEOUT", `build 任务在 ${timeout}ms 后超时`, "sdd build")), timeout);
        }));
    }
    if (signal !== undefined) {
        promises.push(new Promise((_, reject) => {
            if (signal.aborted)
                reject(interruptionError());
            else
                signal.addEventListener("abort", () => reject(interruptionError()), {
                    once: true,
                });
        }));
    }
    return Promise.race(promises);
}
function toLegacyResult(taskId, result) {
    if ("modifiedFiles" in result)
        return result;
    if (result.legacy !== undefined)
        return result.legacy;
    throw new SddError("E_TDD_EVIDENCE_REQUIRED", `任务 ${taskId} 的 v2 执行结果缺少 legacy 证据`, "sdd build");
}
function attachTaskId(taskId, result, legacy) {
    if ("modifiedFiles" in result)
        return { ...legacy, taskId };
    return { ...result, taskId };
}
function readHost(args) {
    return args?.host === "claude-code" ? "claude-code" : "codex";
}
async function readProjectConventionsHash(root) {
    try {
        const profile = JSON.parse(await readFile(join(root, ".sdd", "project", "conventions.json"), "utf8"));
        const stable = { ...profile };
        delete stable.generatedAt;
        delete stable.indexHash;
        return artifactInputHash(stable);
    }
    catch {
        return artifactInputHash("missing-project-conventions");
    }
}
// ── build next / build complete ────────────────────────────────────────────
/**
 * build next：返回下一个待执行任务的 AgentActionRequired。
 *
 * - FileLock（P1-3）
 * - 不覆盖 plan 生成的 Context Pack，只读已有（P1-1）
 * - 标记任务为 BUILDING（P1-2）
 * - 排除已 DONE 或 已 BUILDING 的任务
 */
async function buildNextTask(root, rawArgs) {
    const lock = new FileLock(root);
    await lock.acquire("sdd build next", undefined, lockOptions(rawArgs));
    try {
        const state = await new StateStore(root).read();
        // 无论来自手动 build 还是 auto loop，均从独立 handoff 状态恢复同一任务。
        if (state.currentPhase === "BUILD_WAITING_AGENT") {
            const pending = state.pendingAgentTask;
            if (pending !== null) {
                const existingTaskId = pending.taskId;
                const changeId = requireActiveChangeId(state.currentChangeId, rawArgs);
                const change = join(root, ".sdd", "changes", changeId);
                const tasks = parseTasks(JSON.stringify((await readCompactPlan(change)).tasks));
                const task = tasks.find((t) => t.id === existingTaskId);
                if (task) {
                    await ensureFreshContextPacks(root, changeId, [task], readHost(rawArgs));
                    const contextPackPath = `.sdd/context-packs/${changeId}/${existingTaskId}.md`;
                    await mkdir(join(root, ".sdd", "runs", state.currentRunId ?? "unknown-run", "tasks"), { recursive: true });
                    const actionRequired = {
                        type: "AGENT_TASK_EXECUTION",
                        taskId: existingTaskId,
                        changeId,
                        contextPack: contextPackPath,
                        allowedFiles: task.allowedFiles ?? [],
                        expectedNewFiles: task.expectedNewFiles ?? [],
                        forbiddenFiles: task.forbiddenFiles ?? [],
                        verification: task.verification?.map((cmd) => {
                            const [command, ...rest] = cmd.split(/\s+/);
                            return { command: command, args: rest };
                        }) ?? [],
                        resultFile: pending.resultFile,
                        codebase: {
                            provider: state.codebaseProvider === "codebase-memory-mcp"
                                ? "codebase-memory-mcp"
                                : "fallback-file-scan",
                            degraded: state.degraded,
                        },
                        policyBundle: resolvePolicyBundle({
                            command: "build",
                            phase: state.currentPhase,
                            actionType: "AGENT_TASK_EXECUTION",
                            ...(task.failureContext === undefined
                                ? {}
                                : { failureCode: task.failureContext.errorCode }),
                        }),
                    };
                    return {
                        ok: true,
                        state: "BUILD_WAITING_AGENT",
                        exitCode: 0,
                        actionRequired,
                        next: "sdd build complete",
                    };
                }
            }
        }
        if (state.currentPhase !== "PLAN_READY" &&
            state.currentPhase !== "BUILDING" &&
            state.currentPhase !== "BUILD_WAITING_AGENT") {
            return {
                ok: false,
                state: state.currentPhase,
                exitCode: 3,
                error: {
                    code: "E_INVALID_PHASE_COMMAND",
                    message: `无法在 ${state.currentPhase} 状态下获取构建任务`,
                    next: state.suggestedCommand ?? "sdd plan",
                },
            };
        }
        const changeId = requireActiveChangeId(state.currentChangeId, rawArgs);
        const change = join(root, ".sdd", "changes", changeId);
        const tasks = parseTasks(JSON.stringify((await readCompactPlan(change)).tasks));
        // 查找第一个可执行任务（排除 DONE 和 BUILDING）
        const taskStatuses = state.tasks;
        const nextTask = tasks.find((t) => taskStatuses[t.id] !== "DONE" &&
            taskStatuses[t.id] !== "BUILDING" &&
            t.dependsOn.every((d) => taskStatuses[d] === "DONE"));
        if (!nextTask) {
            const allDone = tasks.every((t) => taskStatuses[t.id] === "DONE");
            if (!allDone) {
                return {
                    ok: true,
                    state: "BUILDING",
                    exitCode: 0,
                    next: "sdd build complete",
                    data: {
                        pendingBuild: Object.entries(taskStatuses)
                            .filter(([, s]) => s === "BUILDING")
                            .map(([id]) => id),
                    },
                };
            }
            return {
                ok: true,
                state: "BUILD_READY",
                exitCode: 0,
                next: "sdd verify",
                data: { allTasksDone: true },
            };
        }
        await ensureFreshContextPacks(root, changeId, [nextTask], readHost(rawArgs));
        const runId = state.currentRunId ?? `run-${Date.now()}`;
        const contextPackPath = `.sdd/context-packs/${changeId}/${nextTask.id}.md`;
        await mkdir(join(root, ".sdd", "context-packs", changeId), {
            recursive: true,
        });
        const resultFile = `.sdd/runs/${runId}/tasks/${nextTask.id}.result.json`;
        await mkdir(join(root, ".sdd", "runs", runId, "tasks"), {
            recursive: true,
        });
        // 标记任务为 BUILDING + 更新状态（P1-2）
        const since = new Date().toISOString();
        const gitBaseline = await new GitInspector(resolveBusinessRoot(root, state)).snapshot();
        if (!gitBaseline.available)
            throw new SddError("E_SECURITY_BLOCKED", "外部 Agent handoff 需要可用的 Git 仓库以建立文件变更基线");
        await new StateStore(root).update((current) => ({
            ...current,
            currentPhase: "BUILD_WAITING_AGENT",
            inProgressPhase: null,
            lastCommand: "sdd build next",
            lastError: null,
            suggestedCommand: "sdd build complete",
            tasks: { ...current.tasks, [nextTask.id]: "BUILDING" },
            pendingAgentTask: {
                taskId: nextTask.id,
                resultFile,
                since,
                gitBaseline: { ...gitBaseline, available: true },
            },
            activeLoop: current.activeLoop !== null
                ? {
                    ...current.activeLoop,
                    status: "WAITING_AGENT",
                    waiting: {
                        reason: "AGENT_TASK_EXECUTION",
                        taskId: nextTask.id,
                        resultFile,
                        since,
                    },
                }
                : current.activeLoop,
        }));
        // 历史 waiting run 的恢复不能猜测 Git 基线。将本次 handoff 的可信
        // 基线与结果文件一起落盘，resume 才能重新建立同一个 pendingAgentTask。
        await writeFile(join(root, ".sdd", "runs", runId, "tasks", `${nextTask.id}.handoff.json`), `${JSON.stringify({
            taskId: nextTask.id,
            resultFile,
            since,
            gitBaseline: { ...gitBaseline, available: true },
        }, null, 2)}\n`, "utf8");
        const actionRequired = {
            type: "AGENT_TASK_EXECUTION",
            taskId: nextTask.id,
            changeId,
            contextPack: contextPackPath,
            allowedFiles: nextTask.allowedFiles ?? [],
            expectedNewFiles: nextTask.expectedNewFiles ?? [],
            forbiddenFiles: nextTask.forbiddenFiles ?? [],
            verification: nextTask.verification?.map((cmd) => {
                const [command, ...rest] = cmd.split(/\s+/);
                return { command: command, args: rest };
            }) ?? [],
            resultFile,
            codebase: {
                provider: state.codebaseProvider === "codebase-memory-mcp"
                    ? "codebase-memory-mcp"
                    : "fallback-file-scan",
                degraded: state.degraded,
            },
            policyBundle: resolvePolicyBundle({
                command: "build",
                phase: state.currentPhase,
                actionType: "AGENT_TASK_EXECUTION",
                ...(nextTask.failureContext === undefined
                    ? {}
                    : { failureCode: nextTask.failureContext.errorCode }),
            }),
        };
        return {
            ok: true,
            state: "BUILD_WAITING_AGENT",
            exitCode: 0,
            actionRequired,
            next: "sdd build complete",
        };
    }
    finally {
        await lock.release();
    }
}
const VALID_TASK_STATUSES = [
    "SUCCEEDED",
    "FAILED",
    "BLOCKED",
    "SKIPPED",
    "DEGRADED",
];
function invalidCompleteResult(message) {
    return {
        ok: false,
        state: "FAILED",
        exitCode: 4,
        error: { code: "E_MISSING_ARTIFACT", message },
    };
}
function parseCompleteResult(taskId, value) {
    if (typeof value !== "object" ||
        value === null ||
        Array.isArray(value) ||
        Object.getPrototypeOf(value) !== Object.prototype)
        return invalidCompleteResult("TaskExecutionResult 结构不合法");
    const result = value;
    const isV2 = result.schemaVersion === "1.2.0" && "fileDelta" in result;
    if (isV2 && !isValidV2Envelope(result))
        return invalidCompleteResult("v2 task execution result 深层结构不合法");
    const legacy = isV2 ? result.legacy : result;
    if (typeof legacy !== "object" || legacy === null || Array.isArray(legacy))
        return invalidCompleteResult("v2 task execution result 必须包含合法 legacy");
    const parsed = legacy;
    const schemaVersion = isV2 ? result.schemaVersion : parsed.schemaVersion;
    const resultTaskId = isV2 ? result.taskId : parsed.taskId;
    const status = isV2 ? result.status : parsed.status;
    if (typeof schemaVersion !== "string" ||
        resultTaskId !== taskId ||
        !VALID_TASK_STATUSES.includes(status) ||
        !Array.isArray(parsed.modifiedFiles) ||
        !parsed.modifiedFiles.every((file) => typeof file === "string") ||
        !Array.isArray(parsed.tddEvidence) ||
        !Array.isArray(parsed.verification) ||
        !parsed.tddEvidence.every((entry) => typeof entry === "object" &&
            entry !== null &&
            !Array.isArray(entry) &&
            typeof entry.phase === "string" &&
            typeof entry.command === "string" &&
            typeof entry.passed === "boolean" &&
            typeof entry.output === "string") ||
        !parsed.verification.every((entry) => typeof entry === "object" &&
            entry !== null &&
            !Array.isArray(entry) &&
            typeof entry.command === "string" &&
            typeof entry.passed === "boolean"))
        return invalidCompleteResult("TaskExecutionResult 字段类型不合法");
    return {
        ...parsed,
        schemaVersion,
        taskId,
        status,
        modifiedFiles: [...parsed.modifiedFiles],
        tddEvidence: [...parsed.tddEvidence],
        verification: [...parsed.verification],
    };
}
/**
 * build complete：验收 Agent 提交的 TaskExecutionResult。
 *
 * 包含结果持久化（P0-6）和状态持久化（P0-7）。
 */
async function buildCompleteTask(root, rawArgs) {
    const taskId = rawArgs?.taskId;
    const rawResult = rawArgs?.result;
    if (typeof taskId !== "string" || rawResult === undefined) {
        return {
            ok: false,
            state: "FAILED",
            exitCode: 2,
            error: {
                code: "E_INVALID_PHASE_COMMAND",
                message: "build complete 需要 --task 和 --result 参数",
            },
        };
    }
    const parsedResult = parseCompleteResult(taskId, rawResult);
    if ("ok" in parsedResult)
        return parsedResult;
    const resultJson = parsedResult;
    const lock = new FileLock(root);
    await lock.acquire("sdd build complete", undefined, lockOptions(rawArgs));
    try {
        const store = new StateStore(root);
        const state = await store.read();
        if (state.currentPhase !== "BUILD_WAITING_AGENT")
            return invalidCompleteResult(`无法在 ${state.currentPhase} 状态执行 build complete`);
        const pending = state.pendingAgentTask;
        if (pending === null ||
            pending.taskId !== taskId ||
            typeof pending.resultFile !== "string" ||
            state.tasks[taskId] !== "BUILDING")
            return {
                ...invalidCompleteResult("当前没有与该任务匹配的 Agent handoff"),
                error: {
                    code: "E_STATE_CORRUPTED",
                    message: "当前没有与该任务匹配的 Agent handoff",
                },
            };
        const changeId = state.currentChangeId;
        if (!changeId) {
            return {
                ok: false,
                state: "FAILED",
                exitCode: 3,
                error: { code: "E_MISSING_CHANGE", message: "缺少当前变更 ID" },
            };
        }
        const change = join(root, ".sdd", "changes", changeId);
        const tasks = parseTasks(JSON.stringify((await readCompactPlan(change)).tasks));
        const task = tasks.find((t) => t.id === taskId);
        if (!task) {
            return {
                ok: false,
                state: "FAILED",
                exitCode: 4,
                error: { code: "E_MISSING_ARTIFACT", message: `任务 ${taskId} 不存在` },
            };
        }
        if (resultJson.status !== "SUCCEEDED") {
            await store.update((current) => ({
                ...current,
                tasks: { ...current.tasks, [taskId]: "FAILED" },
                currentPhase: "FAILED",
                failedCommand: "sdd build complete",
                failedReason: `Agent 返回 ${resultJson.status}`,
                lastError: `任务 ${taskId} 返回状态 ${resultJson.status}`,
                suggestedCommand: "sdd auto --resume",
                pendingAgentTask: null,
                activeLoop: current.activeLoop === null
                    ? null
                    : { ...current.activeLoop, status: "FAILED", waiting: undefined },
            }));
            return {
                ok: false,
                state: "FAILED",
                exitCode: 7,
                error: {
                    code: "E_AGENT_TASK_FAILED",
                    message: `任务 ${taskId} 返回状态 ${resultJson.status}`,
                },
            };
        }
        // 外部 Agent 的文件声明不是安全事实源，必须以 handoff 前后的真实 Git delta 为准。
        const git = new GitInspector(resolveBusinessRoot(root, state));
        const gitAfter = await git.snapshot();
        if (!gitAfter.available)
            throw new SddError("E_SECURITY_BLOCKED", "无法读取 Git 状态，拒绝验收外部 Agent 结果");
        const modifiedFiles = git.delta(pending.gitBaseline, gitAfter);
        const declaredFiles = [...new Set(resultJson.modifiedFiles)];
        const undeclaredFiles = modifiedFiles.filter((file) => !declaredFiles.includes(file));
        const nonexistentClaims = declaredFiles.filter((file) => !modifiedFiles.includes(file));
        if (undeclaredFiles.length > 0 || nonexistentClaims.length > 0) {
            return {
                ok: false,
                state: "FAILED",
                exitCode: 10,
                error: {
                    code: "E_UNDECLARED_FILE_CHANGE",
                    message: [
                        undeclaredFiles.length === 0
                            ? null
                            : `存在未申报修改：${undeclaredFiles.join(", ")}`,
                        nonexistentClaims.length === 0
                            ? null
                            : `申报文件与真实 Git delta 不一致：${nonexistentClaims.join(", ")}`,
                    ]
                        .filter((message) => message !== null)
                        .join("；"),
                },
            };
        }
        try {
            validateTaskFiles(modifiedFiles, {
                allowedFiles: task.allowedFiles ?? [],
                expectedNewFiles: task.expectedNewFiles ?? [],
                forbiddenFiles: task.forbiddenFiles ?? [],
            });
        }
        catch (error) {
            if (error instanceof SddError && error.code === "E_SECURITY_BLOCKED") {
                return {
                    ok: false,
                    state: "FAILED",
                    exitCode: 5,
                    error: { code: "E_SECURITY_BLOCKED", message: error.message },
                };
            }
            throw error;
        }
        const tddEvidence = resultJson.tddEvidence;
        const evidenceFailures = taskEvidenceFailures(task, resultJson);
        if (evidenceFailures.length > 0)
            return {
                ok: false,
                state: "FAILED",
                exitCode: 7,
                error: {
                    code: "E_TDD_EVIDENCE_REQUIRED",
                    message: evidenceFailures.join("；"),
                },
            };
        // TDD evidence 命令安全校验
        const blockedEvidenceCommand = tddEvidence.find((e) => typeof e.command === "string" && !isCommandAllowed(e.command));
        if (blockedEvidenceCommand) {
            return {
                ok: false,
                state: "FAILED",
                exitCode: 5,
                error: {
                    code: "E_SECURITY_BLOCKED",
                    message: `TDD evidence 命令未在允许清单内：${String(blockedEvidenceCommand.command)}`,
                },
            };
        }
        // verification 校验（P1-6）
        const verification = resultJson.verification;
        // verification 命令安全校验
        const blockedVerificationCommand = verification.find((v) => !isCommandAllowed(v.command));
        if (blockedVerificationCommand) {
            return {
                ok: false,
                state: "FAILED",
                exitCode: 5,
                error: {
                    code: "E_SECURITY_BLOCKED",
                    message: `verification 命令未在允许清单内：${blockedVerificationCommand.command}`,
                },
            };
        }
        const failedVerification = verification.filter((v) => !v.passed);
        if (failedVerification.length > 0) {
            return {
                ok: false,
                state: "FAILED",
                exitCode: 7,
                error: {
                    code: "E_VERIFY_FAILED",
                    message: `验证失败: ${failedVerification.map((v) => v.command).join(", ")}`,
                },
            };
        }
        // === 持久化（P0-6）：写入 task-results.json ===
        const taskStatus = "DONE";
        const existingResults = (await readResults(join(change, "task-results.json")));
        const resultEntry = {
            taskId,
            schemaVersion: resultJson.schemaVersion ?? "1.0.0",
            status: taskStatus,
            modifiedFiles,
            createdFiles: resultJson.createdFiles ?? [],
            commandsRun: resultJson.commandsRun ?? [],
            tddEvidence,
            verification,
        };
        const idx = existingResults.findIndex((r) => r.taskId === taskId);
        if (idx >= 0)
            existingResults[idx] = resultEntry;
        else
            existingResults.push(resultEntry);
        const resultsPath = join(change, "task-results.json");
        const temporaryResultsPath = `${resultsPath}.${process.pid}.tmp`;
        await writeFile(temporaryResultsPath, JSON.stringify(existingResults, null, 2), "utf8");
        await rename(temporaryResultsPath, resultsPath);
        // run artifact 保留完整 V2 envelope，并用真实 Git delta 覆盖 Agent 自报 fileDelta。
        const resultFilePath = join(root, pending.resultFile);
        const originalArtifact = rawResult;
        const persistedArtifact = originalArtifact.schemaVersion === "1.2.0" &&
            typeof originalArtifact.fileDelta === "object"
            ? {
                ...originalArtifact,
                fileDelta: classifyFileDelta(pending.gitBaseline, gitAfter, modifiedFiles),
                legacy: {
                    ...originalArtifact.legacy,
                    modifiedFiles,
                },
            }
            : { ...resultJson, modifiedFiles };
        await mkdir(dirname(resultFilePath), { recursive: true });
        await writeFile(resultFilePath, JSON.stringify(persistedArtifact, null, 2), "utf8");
        // === 状态持久化（P0-7）===
        const allDone = tasks.every((t) => t.id === taskId ? taskStatus === "DONE" : state.tasks[t.id] === "DONE");
        await store.update((current) => ({
            ...current,
            tasks: { ...current.tasks, [taskId]: taskStatus },
            currentPhase: allDone
                ? "BUILD_READY"
                : "PLAN_READY",
            inProgressPhase: null,
            failedCommand: null,
            failedReason: null,
            interruptedCommand: null,
            lastCommand: "sdd build complete",
            lastError: null,
            suggestedCommand: allDone ? "sdd verify" : "sdd build next",
            pendingAgentTask: null,
            // 清除 activeLoop.waiting
            activeLoop: current.activeLoop !== null
                ? (() => {
                    const loop = { ...current.activeLoop };
                    loop.status = "RUNNING";
                    delete loop.waiting;
                    return loop;
                })()
                : current.activeLoop,
        }));
        return {
            ok: true,
            state: allDone ? "BUILD_READY" : "PLAN_READY",
            exitCode: 0,
            next: allDone ? "sdd verify" : "sdd build next",
        };
    }
    finally {
        await lock.release();
    }
}
//# sourceMappingURL=build.js.map