import { mkdir, mkdtemp, readFile, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { AuditLogger } from "../src/audit/audit-logger.js";
import { assertSafePath } from "../src/security/path-safety.js";

// 安全测试聚焦路径逃逸、符号链接和审计日志脱敏三类高风险场景。
const roots: string[] = [];

async function temporaryRoot(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "sdd-security-"));
  roots.push(root);
  return root;
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("path safety", () => {
  it("allows repository paths and blocks traversal and .git writes", async () => {
    const root = await temporaryRoot();
    await expect(assertSafePath(root, "src/index.ts")).resolves.toContain(
      "src/index.ts",
    );
    await expect(assertSafePath(root, "../secret.txt")).rejects.toMatchObject({
      code: "E_PATH_OUTSIDE_REPO",
    });
    await expect(assertSafePath(root, ".git/config")).rejects.toMatchObject({
      code: "E_SECURITY_BLOCKED",
    });
  });

  it("blocks symlinks that resolve outside the repository", async () => {
    const root = await temporaryRoot();
    const outside = await temporaryRoot();
    await mkdir(join(root, "src"), { recursive: true });
    await symlink(outside, join(root, "src/external"));

    await expect(
      assertSafePath(root, "src/external/file.ts"),
    ).rejects.toMatchObject({
      code: "E_SYMLINK_BLOCKED",
    });
  });

  it("blocks Windows drive, UNC, and backslash traversal paths on every host OS", async () => {
    const root = await temporaryRoot();
    for (const candidate of [
      "C:\\secret.txt",
      "\\\\server\\share\\secret.txt",
      "..\\secret.txt",
    ]) {
      await expect(assertSafePath(root, candidate)).rejects.toMatchObject({
        code: "E_PATH_OUTSIDE_REPO",
      });
    }
  });
});

describe("AuditLogger", () => {
  it("writes JSON lines while redacting secrets", async () => {
    const root = await temporaryRoot();
    const logger = new AuditLogger(root);
    await logger.write({
      command: "sdd init",
      phase: "INITIALIZING",
      result: "PASS",
      message: "token=secret-value password=hunter2",
    });

    const line = await readFile(join(root, ".sdd/logs/audit.log"), "utf8");
    expect(line).not.toContain("secret-value");
    expect(line).not.toContain("hunter2");
    expect(JSON.parse(line)).toMatchObject({
      command: "sdd init",
      result: "PASS",
    });
  });

  it("rotates the audit log when it exceeds the configured size", async () => {
    const root = await temporaryRoot();
    await mkdir(join(root, ".sdd/logs"), { recursive: true });
    await writeFile(join(root, ".sdd/logs/audit.log"), "x".repeat(128), "utf8");
    const logger = new AuditLogger(root, { maxBytes: 64, retainedFiles: 2 });

    await logger.write({
      command: "sdd status",
      phase: "INDEX_READY",
      result: "PASS",
    });

    expect(
      await readFile(join(root, ".sdd/logs/audit.log.1"), "utf8"),
    ).toHaveLength(128);
  });
});
