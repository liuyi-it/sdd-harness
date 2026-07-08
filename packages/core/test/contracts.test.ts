import { describe, expect, it } from "vitest";

import {
  COMMANDS,
  ERROR_EXIT_CODES,
  PHASES,
  type CommandResult,
} from "../src/contracts.js";

// 契约测试用于保证公开命令、状态枚举和退出码不会无意漂移。
describe("core contracts", () => {
  it("defines the complete public command set", () => {
    expect([...COMMANDS].sort()).toEqual([
      "archive",
      "auto",
      "build",
      "codebase",
      "design",
      "init",
      "new",
      "plan",
      "review",
      "status",
      "verify",
    ]);
  });

  it("defines every stable and in-progress workflow phase", () => {
    expect(PHASES).toContain("DESIGNING");
    expect(PHASES).toContain("PLANNING");
    expect(PHASES).toHaveLength(21);
  });

  it("maps canonical errors to their specified exit codes", () => {
    expect(ERROR_EXIT_CODES.E_NOT_INITIALIZED).toBe(3);
    expect(ERROR_EXIT_CODES.E_UNRESOLVED_BLOCKER).toBe(6);
    expect(ERROR_EXIT_CODES.E_TDD_EVIDENCE_REQUIRED).toBe(7);
    expect(ERROR_EXIT_CODES.E_TIMEOUT).toBe(124);
    expect(ERROR_EXIT_CODES.E_COMPONENT_INTEGRITY_FAILED).toBe(10);
  });

  it("supports a successful command result", () => {
    const result: CommandResult = {
      ok: true,
      state: "INDEX_READY",
      exitCode: 0,
      next: "sdd new",
    };

    expect(result.ok).toBe(true);
  });
});
