import { appendFile, mkdir, readFile, rename, stat, unlink, } from "node:fs/promises";
import { dirname, join } from "node:path";
import { hasBlockingSecretFinding, scanSecretFindings, } from "../security/secrets-scanner.js";
const SENSITIVE_KEYS = new Set([
    "token",
    "password",
    "secret",
    "apiKey",
    "api_key",
    "apikey",
    "authorization",
    "auth",
    "privateKey",
    "private_key",
    "privatekey",
    "accessKey",
    "access_key",
    "databaseUrl",
    "database_url",
]);
export class AuditLogger {
    path;
    maxBytes;
    retainedFiles;
    constructor(root, options = {}) {
        this.path = join(root, ".sdd", "logs", "audit.log");
        this.maxBytes = options.maxBytes ?? 10 * 1024 * 1024;
        this.retainedFiles = options.retainedFiles ?? 5;
    }
    async write(event) {
        await mkdir(dirname(this.path), { recursive: true });
        await this.rotateIfRequired();
        const safeEvent = {
            timestamp: new Date().toISOString(),
            command: event.command,
            phase: event.phase,
            result: event.result,
            changeId: event.changeId,
            ...(event.message === undefined
                ? {}
                : { message: redactText(event.message) }),
            ...(event.meta === undefined ? {} : { meta: redactJson(event.meta) }),
        };
        await appendFile(this.path, `${JSON.stringify(safeEvent)}\n`, "utf8");
    }
    async entries() {
        try {
            return (await readFile(this.path, "utf8"))
                .trim()
                .split("\n")
                .filter(Boolean)
                .map((line) => JSON.parse(line));
        }
        catch {
            return [];
        }
    }
    /**
     * 扫描指定文件并把命中写入审计；原始值不会被落盘。
     * 返回是否发现 private-key / github-token 等不可豁免命中。
     */
    async reportFindings(file, contents) {
        const findings = scanSecretFindings(contents, file);
        if (findings.length === 0)
            return false;
        await this.write({
            command: "secrets-scanner",
            phase: "REVIEWING",
            result: "PASS",
            meta: {
                file,
                findings: findings.map((finding) => ({
                    rule: finding.rule,
                    preview: finding.preview,
                    line: finding.line ?? null,
                })),
                blocking: hasBlockingSecretFinding(findings),
            },
        });
        return hasBlockingSecretFinding(findings);
    }
    async rotateIfRequired() {
        try {
            if ((await stat(this.path)).size < this.maxBytes)
                return;
        }
        catch {
            return;
        }
        await unlink(`${this.path}.${this.retainedFiles}`).catch(() => undefined);
        for (let index = this.retainedFiles - 1; index >= 1; index -= 1) {
            await rename(`${this.path}.${index}`, `${this.path}.${index + 1}`).catch(() => undefined);
        }
        await rename(this.path, `${this.path}.1`);
    }
}
/**
 * 递归对 JSON 节点进行脱敏。命中敏感键或原始值包含常见 secret pattern 都用 [REDACTED] 替代。
 */
export function redactJson(value) {
    if (value === null)
        return null;
    if (Array.isArray(value))
        return value.map((entry) => redactJson(entry));
    if (typeof value === "string") {
        const findings = scanSecretFindings(value);
        if (findings.length === 0)
            return value;
        return "[REDACTED]";
    }
    if (typeof value !== "object")
        return value;
    const out = {};
    for (const [k, v] of Object.entries(value)) {
        if (SENSITIVE_KEYS.has(k)) {
            out[k] = "[REDACTED]";
            continue;
        }
        out[k] = redactJson(v);
    }
    return out;
}
export function redactText(value) {
    const findings = scanSecretFindings(value);
    if (findings.length === 0)
        return value;
    let redacted = value;
    for (const rule of [
        "private-key",
        "github-token",
        "aws-access-key",
        "jwt",
        "authorization",
        "database-url",
        "generic-secret",
    ]) {
        const pattern = new RegExp(secretRegexByRule(rule), "g");
        redacted = redacted.replace(pattern, (match) => {
            if (match.includes("PRIVATE KEY"))
                return "[PRIVATE_KEY_REDACTED]";
            if (match.startsWith("eyJ"))
                return "[JWT_REDACTED]";
            if (match.toLowerCase().startsWith("authorization")) {
                return `Authorization: ${match.split(/\s+/, 2)[1] ?? "REDACTED"}`;
            }
            if (match.length > 4)
                return `${match.slice(0, 2)}…${match.slice(-2)}`;
            return "[REDACTED]";
        });
    }
    return redacted;
}
function secretRegexByRule(rule) {
    switch (rule) {
        case "private-key":
            return "-----BEGIN [A-Z ]*PRIVATE KEY-----[\\s\\S]*?-----END [A-Z ]*PRIVATE KEY-----";
        case "github-token":
            return "\\b(?:gh[pousr]_[A-Za-z0-9]{36,255}|github_pat_[A-Za-z0-9_]{82})\\b";
        case "aws-access-key":
            return "\\bAKIA[0-9A-Z]{16}\\b";
        case "jwt":
            return "\\beyJ[A-Za-z0-9_-]+\\.eyJ[A-Za-z0-9_-]+\\.[A-Za-z0-9_-]+\\b";
        case "authorization":
            return "\\bAuthorization\\s*[=:]\\s*(?:Bearer|Basic|Token)\\s+[A-Za-z0-9._~+/=-]{8,}";
        case "database-url":
            return "\\b(?:postgres|postgresql|mysql|mongodb|redis|amqp)(?:\\+[a-z]+)?:\\/\\/[^\\s:@]+:[^\\s@]+@[^\\s/]+";
        case "generic-secret":
            return "\\b(?:token|password|passwd|secret|api[_-]?key|access[_-]?key)\\s*[=:]\\s*[\"']?[^\\s\"'<>;]{6,}";
    }
    return "";
}
//# sourceMappingURL=audit-logger.js.map