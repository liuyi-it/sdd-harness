import {
  ERROR_EXIT_CODES,
  type CommandError,
  type ErrorCode,
} from "./contracts.js";

/**
 * SddError 是 Core 内部统一异常类型。
 * 每个错误都带有稳定的错误码、退出码以及建议的下一步命令。
 */
export class SddError extends Error {
  readonly code: ErrorCode;
  readonly exitCode: number;
  readonly next: string | undefined;

  constructor(code: ErrorCode, message: string, next?: string) {
    super(message);
    this.name = "SddError";
    this.code = code;
    this.exitCode = ERROR_EXIT_CODES[code];
    this.next = next;
  }

  toCommandError(): CommandError {
    return {
      code: this.code,
      message: this.message,
      ...(this.next === undefined ? {} : { next: this.next }),
    };
  }
}
