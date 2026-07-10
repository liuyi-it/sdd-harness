import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { isAbsolute, join } from "node:path";
import { AuditLogger } from "../audit/audit-logger.js";
import { artifactInputHash } from "../artifacts/artifact-writer.js";
import { readContextPackMetadata, renderContextPack, stripManagedSections, } from "../build/context-pack.js";
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
import { assertChangeWritable, requireActiveChangeId } from "./change-id.js";
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
        const tasks = JSON.parse(await readFile(join(change, "tasks.json"), "utf8"));
        await ensureFreshContextPacks(root, changeId, tasks, readHost(rawArgs));
        const previousResults = await readResults(join(change, "task-results.json"));
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
    if (typeof value === "object" &&
        value !== null &&
        record?.schemaVersion === "1.2.0" &&
        Array.isArray(record.commandEvidence) &&
        typeof record.fileDelta === "object" &&
        record.fileDelta !== null) {
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
    return { taskId: task.id, ...legacy, modifiedFiles };
}
function lockOptions(args) {
    const timeoutMs = timeoutMilliseconds(args);
    return timeoutMs === undefined ? {} : { timeoutMs };
}
async function ensureFreshContextPacks(root, changeId, tasks, host) {
    const [spec, design, impact, tasksMarkdown, rawTasksJson, codebaseSummary] = await Promise.all([
        readFile(join(root, ".sdd", "changes", changeId, "spec.md"), "utf8"),
        readFile(join(root, ".sdd", "changes", changeId, "design.md"), "utf8"),
        readFile(join(root, ".sdd", "changes", changeId, "impact.md"), "utf8"),
        readFile(join(root, ".sdd", "changes", changeId, "tasks.md"), "utf8"),
        readFile(join(root, ".sdd", "changes", changeId, "tasks.json"), "utf8"),
        readFile(join(root, ".sdd", "index", "codebase-summary.md"), "utf8"),
    ]);
    const tasksJson = JSON.stringify(JSON.parse(rawTasksJson), null, 2);
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
        const contextPack = await readFile(path, "utf8");
        const metadata = readContextPackMetadata(contextPack);
        const rules = await resolveProjectRules(root, [...task.allowedFiles, ...task.expectedNewFiles], host);
        if (metadata.codebaseIndexHash !== expectedCodebaseHash ||
            metadata.sourceArtifactHash !== expectedSourceHash ||
            metadata.projectRulesHash !== rules.hash ||
            metadata.projectConventionsHash !== projectConventionsHash) {
            await writeFile(path, renderContextPack({
                body: stripManagedSections(contextPack),
                rules,
                codebaseSummary,
                spec,
                design,
                impact,
                tasksMarkdown,
                tasksJson,
                projectConventionsHash,
            }), "utf8");
        }
    }
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
async function readResults(path) {
    try {
        return JSON.parse(await readFile(path, "utf8"));
    }
    catch {
        return [];
    }
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
        return artifactInputHash(await readFile(join(root, ".sdd", "project", "conventions.json"), "utf8"));
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
        // 如果当前已有 active waiting task，返回同一份 handoff，不重复分配
        if (state.currentPhase === "BUILD_WAITING_AGENT") {
            const activeLoop = state.activeLoop;
            const waiting = activeLoop?.waiting;
            if (waiting?.taskId && waiting?.resultFile) {
                const existingTaskId = waiting.taskId;
                const changeId = requireActiveChangeId(state.currentChangeId, rawArgs);
                const change = join(root, ".sdd", "changes", changeId);
                const tasks = JSON.parse(await readFile(join(change, "tasks.json"), "utf8"));
                const task = tasks.find((t) => t.id === existingTaskId);
                if (task) {
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
                        resultFile: waiting.resultFile,
                        codebase: {
                            provider: state.codebaseProvider === "codebase-memory-mcp"
                                ? "codebase-memory-mcp"
                                : "fallback-file-scan",
                            degraded: state.degraded,
                        },
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
        const tasks = JSON.parse(await readFile(join(change, "tasks.json"), "utf8"));
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
        const runId = state.currentRunId ?? `run-${Date.now()}`;
        const contextPackPath = `.sdd/context-packs/${changeId}/${nextTask.id}.md`;
        await mkdir(join(root, ".sdd", "context-packs", changeId), {
            recursive: true,
        });
        // 不覆盖 plan 生成的 Context Pack（P1-1）
        try {
            await access(join(root, contextPackPath));
        }
        catch {
            await writeFile(join(root, contextPackPath), `# Task: ${nextTask.id}\n\nPhase: ${nextTask.phase}\n\n## Allowed Files\n${(nextTask.allowedFiles ?? []).map((f) => `- ${f}`).join("\n")}\n\n## Description\n${nextTask.title ?? "实施此任务"}\n`, "utf8");
        }
        const resultFile = `.sdd/runs/${runId}/tasks/${nextTask.id}.result.json`;
        await mkdir(join(root, ".sdd", "runs", runId, "tasks"), {
            recursive: true,
        });
        // 标记任务为 BUILDING + 更新状态（P1-2）
        await new StateStore(root).update((current) => ({
            ...current,
            currentPhase: "BUILD_WAITING_AGENT",
            inProgressPhase: null,
            lastCommand: "sdd build next",
            lastError: null,
            suggestedCommand: "sdd build complete",
            tasks: { ...current.tasks, [nextTask.id]: "BUILDING" },
            activeLoop: current.activeLoop !== null && typeof current.activeLoop === "object"
                ? {
                    ...current.activeLoop,
                    status: "WAITING_AGENT",
                    waiting: {
                        reason: "AGENT_TASK_EXECUTION",
                        taskId: nextTask.id,
                        resultFile,
                        since: new Date().toISOString(),
                    },
                }
                : current.activeLoop,
        }));
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
/**
 * build complete：验收 Agent 提交的 TaskExecutionResult。
 *
 * 包含结果持久化（P0-6）和状态持久化（P0-7）。
 */
async function buildCompleteTask(root, rawArgs) {
    const taskId = rawArgs?.taskId;
    const resultJson = rawArgs?.result;
    if (!taskId || !resultJson) {
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
    // Schema 校验（P1-5）
    if (!resultJson.schemaVersion ||
        !resultJson.taskId ||
        !VALID_TASK_STATUSES.includes(resultJson.status)) {
        return {
            ok: false,
            state: "FAILED",
            exitCode: 4,
            error: {
                code: "E_MISSING_ARTIFACT",
                message: "TaskExecutionResult 结构不合法",
            },
        };
    }
    // result 字段类型校验：防止畸形 result 引发非结构化异常
    if ("modifiedFiles" in resultJson &&
        !Array.isArray(resultJson.modifiedFiles)) {
        return {
            ok: false,
            state: "FAILED",
            exitCode: 4,
            error: {
                code: "E_MISSING_ARTIFACT",
                message: "modifiedFiles 必须是数组",
            },
        };
    }
    if ("createdFiles" in resultJson && !Array.isArray(resultJson.createdFiles)) {
        return {
            ok: false,
            state: "FAILED",
            exitCode: 4,
            error: { code: "E_MISSING_ARTIFACT", message: "createdFiles 必须是数组" },
        };
    }
    if ("tddEvidence" in resultJson && !Array.isArray(resultJson.tddEvidence)) {
        return {
            ok: false,
            state: "FAILED",
            exitCode: 4,
            error: { code: "E_MISSING_ARTIFACT", message: "tddEvidence 必须是数组" },
        };
    }
    if ("verification" in resultJson && !Array.isArray(resultJson.verification)) {
        return {
            ok: false,
            state: "FAILED",
            exitCode: 4,
            error: { code: "E_MISSING_ARTIFACT", message: "verification 必须是数组" },
        };
    }
    // taskId 一致性校验
    if (resultJson.taskId !== taskId) {
        return {
            ok: false,
            state: "FAILED",
            exitCode: 4,
            error: {
                code: "E_STATE_CORRUPTED",
                message: `result.taskId (${String(resultJson.taskId)}) 与 --task (${taskId}) 不一致`,
            },
        };
    }
    const lock = new FileLock(root);
    await lock.acquire("sdd build complete", undefined, lockOptions(rawArgs));
    try {
        const store = new StateStore(root);
        const state = await store.read();
        // 如果处于 BUILD_WAITING_AGENT，校验 complete 的是当前等待任务
        if (state.currentPhase === "BUILD_WAITING_AGENT") {
            const activeLoop = state.activeLoop;
            const waiting = activeLoop?.waiting;
            if (waiting?.taskId && waiting.taskId !== taskId) {
                return {
                    ok: false,
                    state: "FAILED",
                    exitCode: 4,
                    error: {
                        code: "E_STATE_CORRUPTED",
                        message: `当前等待任务为 ${String(waiting.taskId)}，不能 complete ${taskId}`,
                    },
                };
            }
        }
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
        const tasks = JSON.parse(await readFile(join(change, "tasks.json"), "utf8"));
        const task = tasks.find((t) => t.id === taskId);
        if (!task) {
            return {
                ok: false,
                state: "FAILED",
                exitCode: 4,
                error: { code: "E_MISSING_ARTIFACT", message: `任务 ${taskId} 不存在` },
            };
        }
        // v2 result 处理（选择方案 B：v2 必须包含 legacy，从 legacy 提取 v1 字段）
        if (resultJson.schemaVersion === "1.2.0" &&
            "fileDelta" in resultJson &&
            !("modifiedFiles" in resultJson)) {
            if (!resultJson.legacy) {
                return {
                    ok: false,
                    state: "FAILED",
                    exitCode: 7,
                    error: {
                        code: "E_TDD_EVIDENCE_REQUIRED",
                        message: "v2 task execution result 必须包含 legacy TDD evidence",
                    },
                };
            }
            // 从 legacy 提取 v1 字段供后续校验使用
            const legacy = resultJson.legacy;
            resultJson.modifiedFiles = legacy.modifiedFiles;
            resultJson.tddEvidence = legacy.tddEvidence;
            resultJson.verification = legacy.verification;
        }
        // 文件范围校验：统一使用 validateTaskFiles
        const modifiedFiles = resultJson.modifiedFiles ?? [];
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
        // TDD evidence 校验（P1-6：空数组也阻断）
        const tddEvidence = resultJson.tddEvidence ?? [];
        // 验证 evidence 结构合法，不强制要求所有 phase
        // 每个任务执行只包含其对应 phase 的证据
        const invalidEvidence = tddEvidence.filter((e) => !e.phase || typeof e.passed !== "boolean");
        if (invalidEvidence.length > 0) {
            return {
                ok: false,
                state: "FAILED",
                exitCode: 7,
                error: {
                    code: "E_TDD_EVIDENCE_REQUIRED",
                    message: `TDD evidence 结构不合法`,
                },
            };
        }
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
        const verification = resultJson.verification ?? [];
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
        const taskStatus = resultJson.status === "SUCCEEDED" || resultJson.status === "DONE"
            ? "DONE"
            : "FAILED";
        const existingResults = [];
        try {
            const raw = await readFile(join(change, "task-results.json"), "utf8");
            existingResults.push(...JSON.parse(raw));
        }
        catch {
            // 文件不存在则使用空数组
        }
        const resultEntry = {
            taskId,
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
        await writeFile(join(change, "task-results.json"), JSON.stringify(existingResults, null, 2), "utf8");
        // 写入 run result artifact
        const runId = state.currentRunId ?? "unknown-run";
        const resultFilePath = join(root, ".sdd", "runs", runId, "tasks", `${taskId}.result.json`);
        await mkdir(join(root, ".sdd", "runs", runId, "tasks"), {
            recursive: true,
        });
        await writeFile(resultFilePath, JSON.stringify(resultJson, null, 2), "utf8");
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
            // 清除 activeLoop.waiting
            activeLoop: current.activeLoop !== null && typeof current.activeLoop === "object"
                ? (() => {
                    const loop = {
                        ...current.activeLoop,
                    };
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