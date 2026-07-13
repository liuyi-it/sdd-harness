import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { resolvePolicyBundle } from "@sdd-harness/agent-policies";
import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter, artifactInputHash, } from "../artifacts/artifact-writer.js";
import { SddError } from "../errors.js";
import { FileLock } from "../state/file-lock.js";
import { StateStore } from "../state/state-store.js";
import { assertChangeWritable, requireActiveChangeId } from "./change-id.js";
import { assertRecoverableCommandState, canResumeCommand, normalizeCommandError, persistCommandFailure, previousStablePhase, } from "./recovery.js";
import { timeoutMilliseconds, withTimeout } from "./timeout.js";
/**
 * design 阶段把 spec、impact 和代码库上下文收束成可执行设计稿。
 * 如果输入未变化，则直接返回 already ready，保持幂等。
 */
export async function runDesign(root, engine, args, signal) {
    const lock = new FileLock(root);
    await lock.acquire("sdd design", undefined, lockOptions(args));
    const store = new StateStore(root);
    let started = false;
    let previousPhase = "SPEC_READY";
    try {
        const state = await store.read();
        const retrying = canResumeCommand(state, "sdd design");
        assertRecoverableCommandState(state, "sdd design");
        previousPhase = previousStablePhase(state, "SPEC_READY");
        if (state.currentPhase !== "SPEC_READY" &&
            state.currentPhase !== "DESIGN_READY" &&
            !retrying) {
            throw new SddError("E_INVALID_PHASE_COMMAND", `无法在 ${state.currentPhase} 状态下执行 design`, state.suggestedCommand ?? undefined);
        }
        const changeId = requireActiveChangeId(state.currentChangeId, args);
        await assertChangeWritable(root, changeId);
        const change = join(root, ".sdd", "changes", changeId);
        await store.update((current) => ({
            ...current,
            currentPhase: "DESIGNING",
            inProgressPhase: "DESIGNING",
            previousPhase,
            lastCommand: "sdd design",
            lastError: null,
        }));
        started = true;
        const spec = await readFile(join(change, "spec.md"), "utf8");
        const impact = await readFile(join(change, "impact.md"), "utf8");
        const input = {
            spec,
            impact,
            codebaseSummary: await readFile(join(root, ".sdd/index/codebase-summary.md"), "utf8"),
            packageStructure: await readFile(join(root, ".sdd/index/package-structure.md"), "utf8"),
            architecture: await readFile(join(root, ".sdd/index/architecture.md"), "utf8"),
            policyBundle: resolvePolicyBundle({
                command: "design",
                phase: "SPEC_READY",
                ...(requiresAlternativeDesign(spec, impact)
                    ? { actionType: "HIGH_RISK_DESIGN" }
                    : {}),
            }),
        };
        const writer = new ArtifactWriter();
        const designPath = join(change, "design.md");
        const inputHash = artifactInputHash(input);
        const force = args?.force === true;
        let existingDesign;
        let unchanged = false;
        try {
            const metadata = JSON.parse(await readFile(`${designPath}.meta.json`, "utf8"));
            if (metadata.inputHash === inputHash) {
                unchanged = true;
            }
            else {
                existingDesign = await readFile(designPath, "utf8");
            }
        }
        catch {
            // 文件不存在 → 正常生成
        }
        if (unchanged) {
            await store.update((current) => ({
                ...current,
                currentPhase: "DESIGN_READY",
                inProgressPhase: null,
                suggestedCommand: "sdd plan",
                artifacts: { ...current.artifacts, design: "READY" },
            }));
            return {
                ok: true,
                state: "DESIGN_READY",
                exitCode: 0,
                changeId,
                next: "sdd plan",
                data: { alreadyReady: true },
            };
        }
        if (!force && existingDesign !== undefined) {
            input.existingDesign = existingDesign;
        }
        const designContent = await withTimeout(Promise.resolve(engine.generateDesign(input)), timeoutMilliseconds(args), "sdd design", signal);
        await writer.write(designPath, designContent, input);
        const ready = await store.update((current) => ({
            ...current,
            currentPhase: "DESIGN_READY",
            inProgressPhase: null,
            failedCommand: null,
            failedReason: null,
            interruptedCommand: null,
            artifacts: { ...current.artifacts, design: "READY" },
            suggestedCommand: "sdd plan",
        }));
        await new AuditLogger(root).write({
            command: "sdd design",
            phase: ready.currentPhase,
            result: "PASS",
            changeId,
        });
        return {
            ok: true,
            state: ready.currentPhase,
            exitCode: 0,
            changeId,
            next: "sdd plan",
        };
    }
    catch (error) {
        const normalized = normalizeCommandError(error, "E_STATE_CORRUPTED", "sdd design");
        if (started) {
            await persistCommandFailure(store, normalized, {
                command: "sdd design",
                previousPhase,
                inProgressPhase: "DESIGNING",
            });
        }
        throw normalized;
    }
    finally {
        await lock.release();
    }
}
function requiresAlternativeDesign(spec, impact) {
    return /(?:公开|public|API|协议|schema|数据库|持久化|迁移|跨包|workspace)/iu.test(`${spec}\n${impact}`);
}
function lockOptions(args) {
    const timeoutMs = timeoutMilliseconds(args);
    return timeoutMs === undefined ? {} : { timeoutMs };
}
//# sourceMappingURL=design.js.map