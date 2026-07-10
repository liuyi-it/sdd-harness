import { ERROR_EXIT_CODES, } from "./contracts.js";
/**
 * SddError 是 Core 内部统一异常类型。
 * 每个错误都带有稳定的错误码、退出码以及建议的下一步命令。
 */
export class SddError extends Error {
    code;
    exitCode;
    next;
    constructor(code, message, next) {
        super(message);
        this.name = "SddError";
        this.code = code;
        this.exitCode = ERROR_EXIT_CODES[code];
        this.next = next;
    }
    toCommandError() {
        return {
            code: this.code,
            message: this.message,
            ...(this.next === undefined ? {} : { next: this.next }),
        };
    }
}
//# sourceMappingURL=errors.js.map