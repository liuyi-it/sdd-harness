import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import {
  AuditLogger,
  redactJson,
  redactText,
} from "../src/audit/audit-logger.js";
import {
  createSecretAllowList,
  hasBlockingSecretFinding,
  scanSecretFindings,
  scanSecrets,
  SECRET_RULES,
} from "../src/security/secrets-scanner.js";

describe("Secrets Scanner v1.2", () => {
  const roots: string[] = [];
  afterEach(async () => {
    const { rm } = await import("node:fs/promises");
    await Promise.all(roots.splice(0).map((r) => rm(r, { recursive: true })));
  });

  it("recognizes every documented rule name", () => {
    expect(SECRET_RULES).toEqual([
      "aws-access-key",
      "github-token",
      "jwt",
      "private-key",
      "authorization",
      "database-url",
      "generic-secret",
    ]);
  });

  it("scans AWS access keys without exposing the full value", () => {
    const sample = "export AWS_KEY=AKIAABCDEFGHIJKLMNOP";
    const findings = scanSecretFindings(sample);
    expect(findings).toHaveLength(1);
    expect(findings[0]?.rule).toBe("aws-access-key");
    expect(findings[0]?.preview.startsWith("AKIA…")).toBe(true);
    expect(JSON.stringify(findings)).not.toContain("AKIAABCDEFGHIJKLMNOP");
  });

  it("scans GitHub tokens and treats them as blocking", () => {
    const sample = "token: ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789";
    const findings = scanSecretFindings(sample);
    expect(findings.some((f) => f.rule === "github-token")).toBe(true);
    expect(hasBlockingSecretFinding(findings)).toBe(true);
  });

  it("scans JWTs and Authorization headers", () => {
    const sample =
      "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.signature";
    const findings = scanSecretFindings(sample);
    const rules = findings.map((f) => f.rule);
    expect(rules).toContain("authorization");
    expect(rules).toContain("jwt");
  });

  it("scans private keys and never returns the original", () => {
    const sample = [
      "-----BEGIN RSA PRIVATE KEY-----",
      "MIIEowIBAAKCAQEA0Z3VS5JJcds3xfn/ygWyF5PBbGPhqUg",
      "-----END RSA PRIVATE KEY-----",
    ].join("\n");
    const findings = scanSecretFindings(sample);
    expect(findings.some((f) => f.rule === "private-key")).toBe(true);
    expect(JSON.stringify(findings)).not.toContain(
      "MIIEowIBAAKCAQEA0Z3VS5JJcds3xfn/ygWyF5PBbGPhqUg",
    );
  });

  it("scans database URLs with embedded password", () => {
    const sample = "DATABASE_URL=postgres://app:s3cret@db.local:5432/main";
    const findings = scanSecretFindings(sample);
    const db = findings.find((f) => f.rule === "database-url");
    expect(db).toBeDefined();
    expect(db?.preview).toContain("***");
  });

  it("scans generic secret assignments", () => {
    const sample = "config = { password: 'hunter2hunter2', token: 'abcd' };";
    const findings = scanSecretFindings(sample);
    expect(findings.some((f) => f.rule === "generic-secret")).toBe(true);
  });

  it("allow list cannot disable private-key and github-token", () => {
    const allow = createSecretAllowList(
      ["private-key", "github-token"],
      ["src/keys.ts"],
    );
    const privateKey =
      "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----";
    const result = scanSecrets({
      text: privateKey,
      file: "src/keys.ts",
      allowList: allow,
    });
    expect(result.findings.some((f) => f.rule === "private-key")).toBe(true);
  });

  it("allow list can suppress generic-secret in a non-secret config file", () => {
    const allow = createSecretAllowList(
      ["generic-secret"],
      ["fixtures/config.json"],
    );
    const text = '{"password": "x".repeat(10)}';
    const result = scanSecrets({
      text,
      file: "fixtures/config.json",
      allowList: allow,
    });
    expect(result.findings).toHaveLength(0);
  });

  it("scanSecrets returns line numbers and timestamps", () => {
    const text = "line one\nAKIAABCDEFGHIJKLMNOP\nline three";
    const result = scanSecrets({ text, file: "src/aws.ts" });
    expect(result.scannedAt).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    const aws = result.findings.find((f) => f.rule === "aws-access-key");
    expect(aws?.line).toBe(2);
  });

  it("redactText keeps non-sensitive content and redacts tokens", () => {
    const safe = "build successful";
    expect(redactText(safe)).toBe(safe);
    const dirty = "Authorization: Bearer abcdefghijklmnop";
    const out = redactText(dirty);
    expect(out).not.toContain("abcdefghijklmnop");
  });

  it("redactJson redacts sensitive keys recursively", () => {
    const value = {
      command: "sdd build",
      token: "should_not_appear",
      nested: { password: "p4ssw0rd" },
      list: [{ secret: "abc123" }, "safe"],
    };
    const redacted = redactJson(value) as Record<string, unknown>;
    expect(redacted.token).toBe("[REDACTED]");
    expect((redacted.nested as Record<string, unknown>).password).toBe(
      "[REDACTED]",
    );
    expect(
      ((redacted.list as unknown[])[0] as Record<string, unknown>).secret,
    ).toBe("[REDACTED]");
    expect((redacted.list as unknown[])[1]).toBe("safe");
  });

  it("AuditLogger write never persists raw secret and reportFindings logs redacted preview", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-secret-"));
    roots.push(root);
    const logger = new AuditLogger(root);
    await logger.write({
      command: "sdd verify",
      phase: "VERIFYING",
      result: "PASS",
      message: "Found AKIAABCDEFGHIJKLMNOP in config",
      meta: {
        token: "ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789",
        password: "supersecret",
      },
    });
    const contents = await readFile(join(root, ".sdd/logs/audit.log"), "utf8");
    expect(contents).not.toContain("AKIAABCDEFGHIJKLMNOP");
    expect(contents).not.toContain("ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789");
    expect(contents).not.toContain("supersecret");

    const fs = await import("node:fs/promises");
    await fs.mkdir(join(root, "src"), { recursive: true });
    const srcPath = join(root, "src", "leak.ts");
    await fs.writeFile(
      srcPath,
      "const k = 'ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789';",
      "utf8",
    );
    const blocked = await logger.reportFindings(
      "src/leak.ts",
      "const k = 'ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789';",
    );
    expect(blocked).toBe(true);
    const log = await readFile(join(root, ".sdd/logs/audit.log"), "utf8");
    expect(log).not.toContain("ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789");
  });
});
