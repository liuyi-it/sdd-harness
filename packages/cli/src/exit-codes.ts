/** CLI 退出码映射表，所有命令必须使用此表中的退出码 */
export const ExitCode = {
  SUCCESS: 0,
  GENERAL_ERROR: 1,
  INVALID_ARGS: 2,
  STATE_CONFLICT: 3,
  SCHEMA_VALIDATION_FAILED: 4,
  SECURITY_BLOCKED: 5,
  COMPONENT_UNAVAILABLE: 6,
  TIMEOUT: 7,
} as const;

export type ExitCodeValue = (typeof ExitCode)[keyof typeof ExitCode];
