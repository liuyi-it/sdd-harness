import { SddError } from "../errors.js";
/**
 * 对阶段内部的异步工作增加统一超时保护。
 * 超时后统一抛出 E_TIMEOUT，交给各命令已有的失败持久化逻辑处理。
 */
export async function withTimeout(work, timeoutMs, command, signal) {
    throwIfAborted(signal, command);
    const guards = [];
    if (timeoutMs !== undefined) {
        guards.push(new Promise((_, reject) => {
            setTimeout(() => {
                reject(new SddError("E_TIMEOUT", `${command} 阶段执行超时`, command));
            }, timeoutMs);
        }));
    }
    if (signal !== undefined) {
        guards.push(new Promise((_, reject) => {
            signal.addEventListener("abort", () => reject(interruptedError(command)), {
                once: true,
            });
        }));
    }
    if (guards.length === 0)
        return await work;
    return await Promise.race([work, ...guards]);
}
export function timeoutMilliseconds(args) {
    const seconds = args?.timeout;
    return typeof seconds === "number" && seconds > 0
        ? seconds * 1_000
        : undefined;
}
export function throwIfAborted(signal, command) {
    if (signal?.aborted === true) {
        throw interruptedError(command);
    }
}
export function interruptedError(command) {
    return new SddError("E_INTERRUPTED", `${command} 已被中断`, command);
}
//# sourceMappingURL=timeout.js.map