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

export function isCommandAllowed(command: string): boolean {
  if (SHELL_OPERATORS.test(command)) return false;
  const tokens = command.trim().split(/\s+/);
  return ALLOWED_PREFIXES.some(
    (prefix) =>
      prefix.length <= tokens.length &&
      prefix.every((token, index) => tokens[index] === token),
  );
}
