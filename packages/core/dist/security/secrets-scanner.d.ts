/**
 * 二期 Secrets Scanner：扫描 current-run diff、任务结果和待写制品；
 * preview 只保留类型与首尾最多 2 字符，原始值不得进入 audit.log 或制品。
 *
 * 不可豁免的规则：private-key、github-token。所有其它规则仅允许按
 * “路径 + 规则类型”配置例外，例外记录由 caller 落审计。
 */
export declare const SECRET_RULES: readonly ["aws-access-key", "github-token", "jwt", "private-key", "authorization", "database-url", "generic-secret"];
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
export declare function createSecretAllowList(rules?: readonly SecretRule[], paths?: readonly string[]): SecretRuleAllowList;
export declare function scanSecrets(input: SecretMatchInput): SecretScanResult;
export declare function scanSecretFindings(text: string, file?: string): SecretFinding[];
export declare function hasBlockingSecretFinding(findings: readonly SecretFinding[]): boolean;
//# sourceMappingURL=secrets-scanner.d.ts.map