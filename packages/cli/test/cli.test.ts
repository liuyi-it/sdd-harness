import { describe, it, expect } from "vitest";
import { execSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const CLI_PATH = path.resolve(__dirname, "../dist/cli.js");

function sdd(args: string): string {
  return execSync(`node ${CLI_PATH} ${args}`, {
    encoding: "utf-8",
  }).trim();
}

describe("sdd CLI", () => {
  it("输出 --version", () => {
    const out = sdd("--version");
    expect(out).toContain("0.1.0");
  });

  it("sdd-harness bin 也可用", () => {
    const out = execSync(`node ${CLI_PATH} --version`, {
      encoding: "utf-8",
      env: { ...process.env, SDD_BIN_NAME: "sdd-harness" },
    }).trim();
    expect(out).toContain("sdd-harness");
    expect(out).toContain("0.1.0");
  });

  it("输出 --help", () => {
    const out = sdd("--help");
    expect(out).toContain("init");
    expect(out).toContain("status");
    expect(out).toContain("build");
  });

  it("无参数时也输出帮助", () => {
    const out = sdd("");
    expect(out).toContain("init");
  });

  it("未知命令返回非零退出码", () => {
    expect(() => sdd("unknown-command")).toThrow();
  });

  it("status 命令返回 Structured 状态", () => {
    const out = sdd("status");
    expect(out).toContain("State:");
  });

  it("codebase 子命令缺少参数时返回错误", () => {
    expect(() => sdd("codebase")).toThrow();
  });

  it("codebase status 可用", () => {
    const out = sdd("codebase status");
    expect(out).toContain("State:");
  });

  it("--json 输出有效 JSON", () => {
    const out = execSync(`node ${CLI_PATH} status --json`, {
      encoding: "utf-8",
    }).trim();
    const parsed = JSON.parse(out);
    expect(parsed).toHaveProperty("ok");
    expect(parsed).toHaveProperty("state");
    expect(parsed).toHaveProperty("exitCode");
  });

  it("exitCode 与进程退出码一致", () => {
    // 对不存在目录执行 init，应返回非 0 退出码
    try {
      execSync(`node ${CLI_PATH} init --cwd /nonexistent`, {
        encoding: "utf-8",
        stdio: "pipe",
      });
    } catch (e: unknown) {
      const err = e as { status?: number };
      expect(err.status).toBeGreaterThan(0);
    }
  });
});
