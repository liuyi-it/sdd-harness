import type { AllowedCommand } from "../build/task-executor.js";

const ALLOWED_PREFIXES = [
  ["git", "status"],
  ["git", "diff"],
  ["git", "log"],
  ["mvn", "test"],
  ["mvn", "verify"],
  ["npm", "test"],
  ["npm", "run", "test"],
] as const;

const SHELL_OPERATORS = /(?:&&|\|\||[|;<>`]|\$\()/;
const UNSAFE_TOKEN = /["'\\]/;
const SAFE_ARG = /^[A-Za-z0-9./:_=@+-]+$/;

export function isCommandAllowed(command: string): boolean {
  try {
    return isAllowedCommand(parseAllowedCommand(command));
  } catch {
    return false;
  }
}

export function parseAllowedCommand(command: string): AllowedCommand {
  if (SHELL_OPERATORS.test(command))
    throw new Error("命令包含不安全 shell 操作符");
  if (UNSAFE_TOKEN.test(command)) throw new Error("命令包含不安全转义字符");
  const tokens = command
    .trim()
    .split(/\s+/)
    .filter((token) => token.length > 0);
  if (tokens.length === 0) throw new Error("命令不能为空");
  if (!tokens.every((token) => SAFE_ARG.test(token)))
    throw new Error("命令参数包含不安全字符");
  return {
    command: tokens[0]!,
    args: tokens.slice(1),
  };
}

export function isAllowedCommand(command: AllowedCommand): boolean {
  const tokens = [command.command, ...command.args];
  return ALLOWED_PREFIXES.some(
    (prefix) =>
      prefix.length <= tokens.length &&
      prefix.every((token, index) => tokens[index] === token),
  );
}
