/**
 * 二期 Secrets Scanner：扫描 current-run diff、任务结果和待写制品；
 * preview 只保留类型与首尾最多 2 字符，原始值不得进入 audit.log 或制品。
 *
 * 不可豁免的规则：private-key、github-token。所有其它规则仅允许按
 * “路径 + 规则类型”配置例外，例外记录由 caller 落审计。
 */

export const SECRET_RULES = [
  "aws-access-key",
  "github-token",
  "jwt",
  "private-key",
  "authorization",
  "database-url",
  "generic-secret",
] as const;

export type SecretRule = (typeof SECRET_RULES)[number];

export interface SecretFinding {
  rule: SecretRule;
  preview: string;
  file?: string;
  line?: number;
  /** raw 必须以红化形式存在（仅类型 + 首尾），original 严禁保留。 */
  redactedValue: string;
}

export interface SecretMatchInput {
  text: string;
  file?: string;
  allowList?: SecretRuleAllowList;
}

export interface SecretRuleAllowList {
  rules: ReadonlySet<SecretRule>;
  paths: ReadonlySet<string>;
}

export interface SecretScanResult {
  findings: SecretFinding[];
  scannedAt: string;
}

const UNCONDITIONAL_BLOCKED: ReadonlySet<SecretRule> = new Set<SecretRule>([
  "private-key",
  "github-token",
]);

const RULE_PATTERNS: Record<SecretRule, RegExp> = {
  "aws-access-key": /\bAKIA[0-9A-Z]{16}\b/g,
  "github-token":
    /\b(?:gh[pousr]_[A-Za-z0-9]{36,255}|github_pat_[A-Za-z0-9_]{82})\b/g,
  jwt: /\beyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b/g,
  "private-key":
    /-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g,
  authorization:
    /\bAuthorization\s*[=:]\s*(?:Bearer|Basic|Token)\s+[A-Za-z0-9._~+/=-]{8,}/gi,
  "database-url":
    /\b(?:postgres|postgresql|mysql|mongodb|redis|amqp)(?:\+[a-z]+)?:\/\/[^\s:@]+:([^\s@]+)@[^\s/]+/gi,
  "generic-secret":
    /\b(?:token|password|passwd|secret|api[_-]?key|access[_-]?key)\s*[=:]\s*["']?([^\s"';<>]{6,})/gi,
};

const RULE_PREVIEWS: Record<SecretRule, (match: string) => string> = {
  "aws-access-key": (m) => `AKIA…${m.slice(-2)}`,
  "github-token": (m) => `${m.slice(0, 3)}…${m.slice(-2)}`,
  jwt: () => "[JWT_REDACTED]",
  "private-key": () => "[PRIVATE_KEY_REDACTED]",
  authorization: (m) => `Authorization: ${m.split(/\s+/, 2)[1] ?? "REDACTED"}`,
  "database-url": (m) => m.replace(/:[^:/@]+@/, ":***@"),
  "generic-secret": (m) => `${m.split(/[=:]/)[0]}=***`,
};

export function createSecretAllowList(
  rules: readonly SecretRule[] = [],
  paths: readonly string[] = [],
): SecretRuleAllowList {
  return {
    rules: new Set<SecretRule>(rules),
    paths: new Set<string>(paths),
  };
}

export function scanSecrets(input: SecretMatchInput): SecretScanResult {
  const findings: SecretFinding[] = [];
  const allow = input.allowList ?? { rules: new Set(), paths: new Set() };
  for (const rule of SECRET_RULES) {
    if (UNCONDITIONAL_BLOCKED.has(rule)) {
      // private-key / github-token 不可被 allowList 关闭；
      // 但路径仍然按 caller 决定是否记录。
    } else if (allow.rules.has(rule) && allow.paths.has(input.file ?? "")) {
      continue;
    }
    const pattern = RULE_PATTERNS[rule];
    for (const match of input.text.matchAll(pattern)) {
      const value = match[0];
      findings.push({
        rule,
        preview: RULE_PREVIEWS[rule](value),
        ...(input.file === undefined ? {} : { file: input.file }),
        ...(typeof match.index === "number"
          ? { line: lineOf(input.text, match.index) }
          : {}),
        redactedValue: redactedShape(value, rule),
      });
    }
  }
  return { findings, scannedAt: new Date().toISOString() };
}

export function scanSecretFindings(
  text: string,
  file?: string,
): SecretFinding[] {
  return scanSecrets({ text, ...(file === undefined ? {} : { file }) })
    .findings;
}

export function hasBlockingSecretFinding(
  findings: readonly SecretFinding[],
): boolean {
  return findings.some((finding) => UNCONDITIONAL_BLOCKED.has(finding.rule));
}

function lineOf(text: string, index: number): number {
  let line = 1;
  for (let i = 0; i < index && i < text.length; i += 1) {
    if (text[i] === "\n") line += 1;
  }
  return line;
}

function redactedShape(value: string, rule: SecretRule): string {
  switch (rule) {
    case "aws-access-key":
    case "github-token":
    case "jwt":
    case "authorization":
    case "database-url":
    case "generic-secret":
      return value.length > 4
        ? `${value.slice(0, 2)}…${value.slice(-2)}`
        : "[REDACTED]";
    case "private-key":
      return "[PRIVATE_KEY_REDACTED]";
  }
}
