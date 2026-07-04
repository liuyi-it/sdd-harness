import { SddError } from "../errors.js";

/**
 * 对阶段内部的异步工作增加统一超时保护。
 * 超时后统一抛出 E_TIMEOUT，交给各命令已有的失败持久化逻辑处理。
 */
export async function withTimeout<T>(
  work: Promise<T>,
  timeoutMs: number | undefined,
  command: string,
  signal?: AbortSignal,
): Promise<T> {
  throwIfAborted(signal, command);
  const guards: Array<Promise<never>> = [];
  if (timeoutMs !== undefined) {
    guards.push(
      new Promise<never>((_, reject) => {
        setTimeout(() => {
          reject(new SddError("E_TIMEOUT", `${command} 阶段执行超时`, command));
        }, timeoutMs);
      }),
    );
  }
  if (signal !== undefined) {
    guards.push(
      new Promise<never>((_, reject) => {
        signal.addEventListener(
          "abort",
          () => reject(interruptedError(command)),
          {
            once: true,
          },
        );
      }),
    );
  }
  if (guards.length === 0) return await work;
  return await Promise.race([work, ...guards]);
}

export function timeoutMilliseconds(
  args: Record<string, unknown> | undefined,
): number | undefined {
  const seconds = args?.timeout;
  return typeof seconds === "number" && seconds > 0
    ? seconds * 1_000
    : undefined;
}

export function throwIfAborted(
  signal: AbortSignal | undefined,
  command: string,
): void {
  if (signal?.aborted === true) {
    throw interruptedError(command);
  }
}

export function interruptedError(command: string): SddError {
  return new SddError("E_INTERRUPTED", `${command} 已被中断`, command);
}
