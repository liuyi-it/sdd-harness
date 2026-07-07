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

  it("已知命令返回成功退出码（骨架未实现）", () => {
    const out = sdd("status");
    expect(out).toContain("not yet implemented");
  });
});
