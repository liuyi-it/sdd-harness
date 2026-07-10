import { SddError } from "../errors.js";
/**
 * 对阶段内部的异步工作增加统一超时保护。
 * 超时后统一抛出 E_TIMEOUT，交给各命令已有的失败持久化逻辑处理。
 */
export declare function withTimeout<T>(work: Promise<T>, timeoutMs: number | undefined, command: string, signal?: AbortSignal): Promise<T>;
export declare function timeoutMilliseconds(args: Record<string, unknown> | undefined): number | undefined;
export declare function throwIfAborted(signal: AbortSignal | undefined, command: string): void;
export declare function interruptedError(command: string): SddError;
//# sourceMappingURL=timeout.d.ts.map