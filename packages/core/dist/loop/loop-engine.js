import { COMMANDS } from "../contracts.js";
import { SddError } from "../errors.js";
import { FileLock } from "../state/file-lock.js";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { decide } from "./loop-decision.js";
import { createDefaultLoopSpec } from "./loop-spec.js";
import { runStatus } from "../commands/status.js";
const COMMAND_BY_PHASE = {
    INDEX_READY: "new",
    SPEC_READY: "design",
    DESIGN_READY: "plan",
    PLAN_READY: "build",
    BUILD_WAITING_AGENT: "build",
    BUILD_READY: "verify",
    VERIFY_READY: "review",
    REVIEW_READY: "archive",
};
export class LoopEngine {
    root;
    store;
    loops;
    events;
    execute;
    constructor(root, store, loops, events, execute) {
        this.root = root;
        this.store = store;
        this.loops = loops;
        this.events = events;
        this.execute = execute;
    }
    async run(request) {
        const args = request.args ?? {};
        // resume 与 restart 不能同时使用
        if ((typeof args.resume === "string" || args.resume === true) &&
            args.restart === true) {
            throw new SddError("E_INVALID_PHASE_COMMAND", "resume 与 restart 不能同时使用", "sdd auto");
        }
        if (args.events === true) {
            return this.getEvents(request);
        }
        if (args.loopStatus === true) {
            return this.getLoopStatus();
        }
        if (args.stop === true) {
            return this.stopAuto();
        }
        if (args.restart === true) {
            return this.restartAuto(request);
        }
        if (typeof args.resume === "string" || args.resume === true) {
            return this.resumeAuto(request);
        }
        return this.runAuto(request);
    }
    async runAuto(request) {
        let status = await runStatus(this.root);
        if (status.state === "NOT_INITIALIZED") {
            throw new SddError("E_NOT_INITIALIZED", "请先运行 sdd init 再执行 sdd auto", "sdd init");
        }
        const loop = await this.prepareLoop();
        const spec = await this.readLoopSpec();
        const retries = new Map();
        let noProgressCount = 0;
        await this.events.write(loop.runId, {
            loopId: loop.loopId,
            runId: loop.runId,
            type: "LOOP_STARTED",
            phase: status.state,
        });
        for (let step = 0; step < loop.maxSteps; step += 1) {
            if (status.state === "ARCHIVED") {
                await this.finalizeLoop(loop.runId, "ARCHIVED");
                await this.events.write(loop.runId, {
                    loopId: loop.loopId,
                    runId: loop.runId,
                    type: "LOOP_ARCHIVED",
                    phase: "ARCHIVED",
                });
                return status;
            }
            const command = this.autoCommand(status, request.args);
            if (command === undefined) {
                if (status.state === "CLARIFYING" || status.state === "PAUSED")
                    await this.finalizeLoop(loop.runId, "PAUSED");
                else if (status.state === "FAILED")
                    await this.finalizeLoop(loop.runId, "FAILED");
                return status;
            }
            const startedAt = new Date().toISOString();
            const effectiveArgs = command === "build"
                ? { ...request.args, subcommand: "next" }
                : request.args;
            await this.events.write(loop.runId, {
                loopId: loop.loopId,
                runId: loop.runId,
                type: "COMMAND_STARTED",
                phase: status.state,
                command,
            });
            const result = await this.execute({
                command,
                cwd: this.root,
                ...(effectiveArgs === undefined ? {} : { args: effectiveArgs }),
                ...(request.signal === undefined ? {} : { signal: request.signal }),
            });
            await this.events.write(loop.runId, {
                loopId: loop.loopId,
                runId: loop.runId,
                type: "COMMAND_FINISHED",
                phase: result.state,
                command,
            });
            const decision = decide({ result });
            await this.events.write(loop.runId, {
                loopId: loop.loopId,
                runId: loop.runId,
                type: "DECISION_MADE",
                decision,
                phase: result.state,
            });
            await this.recordStep(loop.runId, {
                kind: decision === "PAUSE_FOR_AGENT" ? "AGENT_HANDOFF" : "COMMAND",
                command,
                phaseBefore: status.state,
                phaseAfter: result.state,
                status: !result.ok
                    ? "FAILED"
                    : decision === "PAUSE_FOR_AGENT"
                        ? "WAITING_AGENT"
                        : decision === "DONE"
                            ? "SUCCEEDED"
                            : "SUCCEEDED",
                decision,
                actionRequired: result.actionRequired,
                startedAt,
                endedAt: new Date().toISOString(),
            });
            noProgressCount = result.state === status.state ? noProgressCount + 1 : 0;
            if (noProgressCount > spec.maxRepeatedFailures)
                throw new SddError("E_STATE_CORRUPTED", `auto 检测到连续 ${noProgressCount} 次无阶段进展，已停止以避免无限循环`, "sdd status");
            if (!result.ok) {
                const attempts = (retries.get(command) ?? 0) + 1;
                retries.set(command, attempts);
                if (attempts <= spec.maxRetriesPerStep) {
                    status = result;
                    continue;
                }
            }
            if (!result.ok ||
                decision === "PAUSE_FOR_AGENT" ||
                decision === "PAUSE_FOR_CLARIFICATION" ||
                decision === "PAUSE_FOR_HUMAN" ||
                decision === "FAIL" ||
                decision === "DONE") {
                const finalStatus = decision === "DONE"
                    ? "ARCHIVED"
                    : decision === "PAUSE_FOR_AGENT"
                        ? "WAITING_AGENT"
                        : decision === "PAUSE_FOR_CLARIFICATION" ||
                            decision === "PAUSE_FOR_HUMAN"
                            ? "PAUSED"
                            : "FAILED";
                if (decision === "PAUSE_FOR_AGENT") {
                    await this.events.write(loop.runId, {
                        loopId: loop.loopId,
                        runId: loop.runId,
                        type: "LOOP_PAUSED",
                    });
                }
                await this.finalizeLoop(loop.runId, finalStatus);
                return result;
            }
            status = result;
        }
        throw new SddError("E_STATE_CORRUPTED", "auto 流程超过了允许的最大阶段推进次数");
    }
    async resumeAuto(request) {
        const args = request.args ?? {};
        const resumeRunId = typeof args.resume === "string" ? args.resume : undefined;
        // 只在需要修改 state/loop run 时才持锁，释放锁后再进入 runAuto
        if (resumeRunId !== undefined) {
            const lock = new FileLock(this.root);
            await lock.acquire("sdd auto --resume");
            try {
                const currentState = await this.store.read();
                const currentLoop = currentState.activeLoop;
                // 恢复到当前 active run 时保留 waiting；历史 waiting 必须从持久化
                // handoff 恢复，不能把一个没有 pending 的 run 静默标记为 RUNNING。
                let keepWaiting = currentLoop !== null && currentLoop.runId === resumeRunId
                    ? currentLoop.waiting
                    : undefined;
                const run = await this.loops.readRun(resumeRunId);
                if (run.status === "ABORTED")
                    throw new SddError("E_INVALID_PHASE_COMMAND", "已终止的 run 不能恢复，请使用 sdd auto --restart", "sdd auto --restart");
                const recoveredPending = keepWaiting === undefined
                    ? await this.recoverHistoricalHandoff(run)
                    : undefined;
                if (recoveredPending !== undefined) {
                    keepWaiting = {
                        reason: "AGENT_TASK_EXECUTION",
                        taskId: recoveredPending.taskId,
                        resultFile: recoveredPending.resultFile,
                        since: recoveredPending.since,
                    };
                }
                if (run.status === "WAITING_AGENT" && keepWaiting === undefined)
                    throw new SddError("E_STATE_CORRUPTED", `无法恢复历史 waiting run ${run.runId} 的 handoff`, "sdd auto --restart");
                const resumedRun = { ...run };
                delete resumedRun.endedAt;
                await this.loops.writeRun({
                    ...resumedRun,
                    status: "RUNNING",
                    updatedAt: new Date().toISOString(),
                });
                const activeLoop = {
                    loopId: run.loopId,
                    runId: run.runId,
                    status: "RUNNING",
                };
                if (keepWaiting !== undefined) {
                    activeLoop.waiting = keepWaiting;
                }
                await this.store.update((current) => ({
                    ...current,
                    currentRunId: run.runId,
                    activeLoop,
                    ...(recoveredPending === undefined
                        ? {}
                        : {
                            currentPhase: "BUILD_WAITING_AGENT",
                            pendingAgentTask: recoveredPending,
                            tasks: {
                                ...current.tasks,
                                [recoveredPending.taskId]: "BUILDING",
                            },
                            suggestedCommand: "sdd build complete",
                        }),
                }));
                await this.events.write(run.runId, {
                    loopId: run.loopId,
                    runId: run.runId,
                    type: "LOOP_RESUMED",
                });
            }
            finally {
                await lock.release();
            }
        }
        return this.runAuto(request);
    }
    async restartAuto(request) {
        const lock = new FileLock(this.root);
        await lock.acquire("sdd auto --restart");
        try {
            const state = await this.store.read();
            if (state.activeLoop !== null) {
                const activeLoop = state.activeLoop;
                try {
                    const existing = await this.loops.readRun(activeLoop.runId);
                    await this.loops.writeRun({
                        ...existing,
                        status: "ABORTED",
                        endedAt: new Date().toISOString(),
                        updatedAt: new Date().toISOString(),
                    });
                }
                catch {
                    // run 不存在，跳过
                }
                await this.events.write(activeLoop.runId, {
                    loopId: activeLoop.loopId,
                    runId: activeLoop.runId,
                    type: "LOOP_STOPPED",
                });
            }
            const spec = await this.readLoopSpec();
            const runId = `run-${Date.now()}`;
            const loopId = state.activeLoop !== null && typeof state.activeLoop === "object"
                ? state.activeLoop.loopId
                : spec.loopId;
            await this.loops.writeRun({
                schemaVersion: "1.3.0",
                runId,
                loopId,
                status: "RUNNING",
                startedAt: new Date().toISOString(),
                updatedAt: new Date().toISOString(),
                currentStep: 0,
                steps: [],
            });
            await this.store.update((current) => ({
                ...current,
                currentRunId: runId,
                activeLoop: { loopId, runId, status: "RUNNING" },
                ...(current.pendingAgentTask === null
                    ? {}
                    : {
                        pendingAgentTask: null,
                        currentPhase: "PLAN_READY",
                        tasks: {
                            ...current.tasks,
                            [current.pendingAgentTask.taskId]: "PENDING",
                        },
                        suggestedCommand: "sdd build next",
                    }),
            }));
            await this.events.write(runId, {
                loopId,
                runId,
                type: "LOOP_RESTARTED",
            });
        }
        finally {
            await lock.release();
        }
        return this.runAuto({
            ...request,
            args: { ...request.args, restart: undefined },
        });
    }
    async stopAuto() {
        const lock = new FileLock(this.root);
        await lock.acquire("sdd auto --stop");
        try {
            const state = await this.store.read();
            if (state.activeLoop === null || typeof state.activeLoop !== "object") {
                return {
                    ok: false,
                    state: "FAILED",
                    exitCode: 3,
                    error: {
                        code: "E_INVALID_PHASE_COMMAND",
                        message: "没有 active loop",
                    },
                };
            }
            const activeLoop = state.activeLoop;
            try {
                const run = await this.loops.readRun(activeLoop.runId);
                await this.loops.writeRun({
                    ...run,
                    status: "ABORTED",
                    endedAt: new Date().toISOString(),
                    updatedAt: new Date().toISOString(),
                });
            }
            catch {
                // run 不存在，跳过
            }
            await this.store.update((current) => ({
                ...current,
                activeLoop: {
                    loopId: activeLoop.loopId,
                    runId: activeLoop.runId,
                    status: "ABORTED",
                },
                ...(current.pendingAgentTask === null
                    ? {}
                    : {
                        pendingAgentTask: null,
                        currentPhase: "PLAN_READY",
                        tasks: {
                            ...current.tasks,
                            [current.pendingAgentTask.taskId]: "PENDING",
                        },
                        suggestedCommand: "sdd build next",
                    }),
            }));
            await this.events.write(activeLoop.runId, {
                loopId: activeLoop.loopId,
                runId: activeLoop.runId,
                type: "LOOP_STOPPED",
            });
            const currentState = await this.store.read();
            return {
                ok: true,
                state: currentState.currentPhase,
                exitCode: 0,
                data: { loopStopped: activeLoop.runId },
            };
        }
        finally {
            await lock.release();
        }
    }
    async getEvents(request) {
        const state = await this.store.read();
        if (state.activeLoop === null || typeof state.activeLoop !== "object") {
            return {
                ok: true,
                state: state.currentPhase,
                exitCode: 0,
                data: { events: [] },
            };
        }
        const activeLoop = state.activeLoop;
        const tail = typeof request.args?.tail === "number" ? request.args.tail : undefined;
        const events = await this.events.read(activeLoop.runId, tail !== undefined ? { tail } : undefined);
        return {
            ok: true,
            state: state.currentPhase,
            exitCode: 0,
            data: { events },
        };
    }
    async getLoopStatus() {
        const state = await this.store.read();
        if (state.activeLoop === null || typeof state.activeLoop !== "object") {
            return {
                ok: true,
                state: state.currentPhase,
                exitCode: 0,
                data: { activeLoop: null },
            };
        }
        const activeLoop = state.activeLoop;
        return {
            ok: true,
            state: state.currentPhase,
            exitCode: 0,
            data: {
                activeLoop: {
                    loopId: activeLoop.loopId,
                    runId: activeLoop.runId,
                    status: activeLoop.status,
                    lastDecision: activeLoop.lastDecision,
                    waiting: activeLoop.waiting,
                    currentPhase: state.currentPhase,
                    nextAction: state.suggestedCommand,
                },
            },
        };
    }
    // ── private helpers ──────────────────────────────────────────────
    autoCommand(status, args) {
        if (status.state === "CLARIFYING") {
            return hasAnswers(args) ? "new" : undefined;
        }
        if (status.state === "FAILED" || status.state === "PAUSED") {
            const stateData = status.data;
            return parseCommandName(stateData?.interruptedCommand ??
                stateData?.failedCommand ??
                stateData?.suggestedCommand ??
                status.next);
        }
        return COMMAND_BY_PHASE[status.state];
    }
    async prepareLoop() {
        const state = await this.store.read();
        const spec = await this.readLoopSpec();
        const currentLoop = state.activeLoop !== null &&
            typeof state.activeLoop === "object" &&
            "runId" in state.activeLoop
            ? state.activeLoop
            : null;
        const runId = currentLoop?.runId ?? state.currentRunId ?? `run-${Date.now()}`;
        if (currentLoop?.status === "ABORTED")
            throw new SddError("E_INVALID_PHASE_COMMAND", "当前 run 已终止，请使用 sdd auto --restart", "sdd auto --restart");
        const loopId = currentLoop?.loopId ?? spec.loopId;
        if (!(await this.loops.hasRun(runId))) {
            await this.loops.writeRun({
                schemaVersion: "1.3.0",
                runId,
                loopId,
                status: "RUNNING",
                startedAt: new Date().toISOString(),
                updatedAt: new Date().toISOString(),
                currentStep: 0,
                steps: [],
            });
        }
        if (currentLoop === null) {
            await this.store.update((current) => ({
                ...current,
                currentRunId: runId,
                activeLoop: {
                    loopId,
                    runId,
                    status: "RUNNING",
                },
            }));
        }
        return { loopId, runId, maxSteps: spec.maxSteps };
    }
    async recordStep(runId, step) {
        const run = await this.loops.readRun(runId);
        await this.loops.writeRun({
            ...run,
            updatedAt: new Date().toISOString(),
            currentStep: run.steps.length + 1,
            ...(step.decision !== undefined ? { lastDecision: step.decision } : {}),
            steps: [
                ...run.steps,
                {
                    step: run.steps.length + 1,
                    kind: step.kind,
                    command: step.command,
                    phaseBefore: step.phaseBefore,
                    ...(step.phaseAfter !== undefined
                        ? { phaseAfter: step.phaseAfter }
                        : {}),
                    status: step.status,
                    ...(step.decision !== undefined ? { decision: step.decision } : {}),
                    ...(step.actionRequired !== undefined
                        ? { actionRequired: step.actionRequired }
                        : {}),
                    startedAt: step.startedAt,
                    endedAt: step.endedAt,
                },
            ],
            status: step.status === "FAILED"
                ? "FAILED"
                : step.status === "WAITING_AGENT"
                    ? "WAITING_AGENT"
                    : "RUNNING",
        });
    }
    async finalizeLoop(runId, status) {
        try {
            const run = await this.loops.readRun(runId);
            await this.loops.writeRun({
                ...run,
                status,
                updatedAt: new Date().toISOString(),
                ...(status === "ARCHIVED" || status === "FAILED"
                    ? { endedAt: new Date().toISOString() }
                    : {}),
            });
        }
        catch {
            // run 不存在时跳过
        }
        await this.store.update((current) => ({
            ...current,
            activeLoop: current.activeLoop === null
                ? current.activeLoop
                : {
                    ...current.activeLoop,
                    runId,
                    status: status === "ARCHIVED" ? "SUCCEEDED" : status,
                },
        }));
    }
    async readLoopSpec() {
        try {
            return await this.loops.readSpec();
        }
        catch {
            return createDefaultLoopSpec();
        }
    }
    async recoverHistoricalHandoff(run) {
        if (run.status !== "WAITING_AGENT")
            return undefined;
        const handoff = [...run.steps]
            .reverse()
            .find((step) => step.kind === "AGENT_HANDOFF" &&
            step.status === "WAITING_AGENT" &&
            step.actionRequired?.type === "AGENT_TASK_EXECUTION");
        const taskId = handoff?.actionRequired?.taskId;
        const resultFile = handoff?.actionRequired?.resultFile;
        if (taskId === undefined || resultFile === undefined)
            return undefined;
        try {
            const raw = JSON.parse(await readFile(join(this.root, ".sdd", "runs", run.runId, "tasks", `${taskId}.handoff.json`), "utf8"));
            if (raw.taskId === taskId &&
                raw.resultFile === resultFile &&
                raw.gitBaseline.available)
                return raw;
        }
        catch {
            return undefined;
        }
        return undefined;
    }
}
function hasAnswers(args) {
    const answers = args?.answers;
    return (answers !== undefined &&
        typeof answers === "object" &&
        answers !== null &&
        Object.keys(answers).length > 0);
}
function parseCommandName(input) {
    if (input === undefined || input === null)
        return undefined;
    const normalized = input.replace(/^\/?sdd[.\s]/, "");
    return COMMANDS.includes(normalized)
        ? normalized
        : undefined;
}
//# sourceMappingURL=loop-engine.js.map