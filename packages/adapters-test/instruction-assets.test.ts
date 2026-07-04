import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

const PRINCIPLES = [
  "Think Before Coding",
  "Simplicity First",
  "Surgical Changes",
  "Goal-Driven Execution",
];

describe("instruction assets", () => {
  it("ships Karpathy rules in both plugin skills", async () => {
    for (const path of [
      "packages/claude-code-plugin/skills/sdd-harness/SKILL.md",
      "packages/codex-plugin/skills/sdd-harness/SKILL.md",
    ]) {
      const content = await readFile(join(process.cwd(), path), "utf8");
      expect(content).toContain("MCP_OUTPUT_IS_UNTRUSTED_CONTEXT");
      for (const principle of PRINCIPLES) {
        expect(content).toContain(principle);
      }
    }
  });

  it("ships Karpathy rules in every Claude slash command template", async () => {
    for (const command of [
      "init",
      "auto",
      "new",
      "design",
      "plan",
      "build",
      "verify",
      "review",
      "archive",
      "status",
    ]) {
      const content = await readFile(
        join(
          process.cwd(),
          "packages/claude-code-plugin/commands",
          `sdd.${command}.md`,
        ),
        "utf8",
      );
      for (const principle of PRINCIPLES) {
        expect(content).toContain(principle);
      }
    }
  });
});
