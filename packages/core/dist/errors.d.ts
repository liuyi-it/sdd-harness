import { type CommandError, type ErrorCode } from "./contracts.js";
/**
 * SddError 是 Core 内部统一异常类型。
 * 每个错误都带有稳定的错误码、退出码以及建议的下一步命令。
 */
export declare class SddError extends Error {
    readonly code: ErrorCode;
    readonly exitCode: number;
    readonly next: string | undefined;
    constructor(code: ErrorCode, message: string, next?: string);
    toCommandError(): CommandError;
}
//# sourceMappingURL=errors.d.ts.map