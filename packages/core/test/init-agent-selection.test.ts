import { access, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import type { AdapterManifest } from "../src/adapters/types.js";
import { installProjectIntegration } from "../src/install/project-installer.js";

const roots: string[] = [];

async function project(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "sdd-agent-sel-"));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Fixture\n", "utf8");
  await writeFile(join(root, "package.json"), '{"name":"fixture"}\n', "utf8");
  return root;
}

afterEach(async () => {
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

function makeClaudeManifest(): AdapterManifest {
  return {
    agent: "claude",
    instructionFile: "CLAUDE.md",
    instructionContent:
      "<!-- sdd-harness:managed -->\n## sdd-harness\n\nClaude 指令内容\n",
    commandsDir: ".claude/commands",
    commandTemplate:
      "---\ndescription: sdd {command}\n---\n\n执行 /sdd.{command}\n",
    skillsDir: ".claude/skills/sdd-harness",
    skillContent: "---\nname: sdd-harness\n---\n\nClaude skill\n",
  };
}

function makeCodexManifest(): AdapterManifest {
  return {
    agent: "codex",
    instructionFile: "AGENTS.md",
    instructionContent:
      "<!-- sdd-harness:managed -->\n## sdd-harness\n\nCodex 指令内容\n",
    commandsDir: ".codex/commands",
    commandTemplate:
      "---\ndescription: sdd {command}\n---\n\n执行 sdd {command}\n",
    skillsDir: ".codex/skills/sdd-harness",
    skillContent: "---\nname: sdd-harness\n---\n\nCodex skill\n",
  };
}

describe("installProjectIntegration agent selection", () => {
  it("仅安装选中的适配器 — claude", async () => {
    const root = await project();
    const result = await installProjectIntegration(root, [
      makeClaudeManifest(),
    ]);

    expect(result.candidateFiles).toEqual([]);
    await expect(access(join(root, "CLAUDE.md"))).resolves.toBeUndefined();
    await expect(access(join(root, "AGENTS.md"))).rejects.toThrow();
    await expect(
      access(join(root, ".claude/commands/sdd.init.md")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(root, ".codex/commands/sdd.init.md")),
    ).rejects.toThrow();
    await expect(
      access(join(root, ".claude/skills/sdd-harness/SKILL.md")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(root, ".codex/skills/sdd-harness/SKILL.md")),
    ).rejects.toThrow();
  });

  it("同时安装 claude 和 codex", async () => {
    const root = await project();
    const result = await installProjectIntegration(root, [
      makeClaudeManifest(),
      makeCodexManifest(),
    ]);

    expect(result.candidateFiles).toEqual([]);
    await expect(access(join(root, "CLAUDE.md"))).resolves.toBeUndefined();
    await expect(access(join(root, "AGENTS.md"))).resolves.toBeUndefined();
    await expect(
      access(join(root, ".claude/commands/sdd.init.md")),
    ).resolves.toBeUndefined();
    await expect(
      access(join(root, ".codex/commands/sdd.init.md")),
    ).resolves.toBeUndefined();
  });

  it("空 manifests 数组不创建任何指令文件", async () => {
    const root = await project();
    const result = await installProjectIntegration(root, []);

    expect(result.candidateFiles).toEqual([]);
    await expect(access(join(root, "CLAUDE.md"))).rejects.toThrow();
    await expect(access(join(root, "AGENTS.md"))).rejects.toThrow();
    // schemas 始终安装
    await expect(
      access(join(root, ".sdd/schemas/config.schema.json")),
    ).resolves.toBeUndefined();
  });

  it("追加模式下已存在的行不会重复", async () => {
    const root = await project();
    await writeFile(
      join(root, "CLAUDE.md"),
      "<!-- sdd-harness:managed -->\n## sdd-harness\n\nClaude 指令内容\n",
      "utf8",
    );

    const result = await installProjectIntegration(root, [
      makeClaudeManifest(),
    ]);

    expect(result.candidateFiles).toEqual([]);
    const content = await readFile(join(root, "CLAUDE.md"), "utf8");
    const lines = content.split("\n");
    const instructionLines = lines.filter((l) => l === "Claude 指令内容");
    expect(instructionLines.length).toBe(1);
  });

  it("追加模式：部分行已存在时仅追加新行", async () => {
    const root = await project();
    await writeFile(
      join(root, "CLAUDE.md"),
      "<!-- sdd-harness:managed -->\n## sdd-harness\n",
      "utf8",
    );

    await installProjectIntegration(root, [makeClaudeManifest()]);

    const content = await readFile(join(root, "CLAUDE.md"), "utf8");
    expect(content).toContain("## sdd-harness");
    expect(content).toContain("Claude 指令内容");
    const h2Count = content
      .split("\n")
      .filter((l) => l === "## sdd-harness").length;
    expect(h2Count).toBe(1);
  });

  it("有 rules 的 manifest 会安装 rule 文件", async () => {
    const root = await project();
    const manifestWithRules: AdapterManifest = {
      ...makeClaudeManifest(),
      rules: [{ file: ".claude/rules/sdd.md", content: "# SDD Rules\n" }],
    };

    await installProjectIntegration(root, [manifestWithRules]);

    const ruleContent = await readFile(
      join(root, ".claude/rules/sdd.md"),
      "utf8",
    );
    expect(ruleContent).toContain("# SDD Rules");
  });

  it("force 模式覆盖已有指令文件", async () => {
    const root = await project();
    await writeFile(join(root, "CLAUDE.md"), "旧内容\n", "utf8");

    await installProjectIntegration(root, [makeClaudeManifest()], {
      force: true,
    });

    const content = await readFile(join(root, "CLAUDE.md"), "utf8");
    expect(content).toContain("<!-- sdd-harness:managed -->");
    expect(content).toContain("Claude 指令内容");
    expect(content).not.toContain("旧内容");
  });
});

describe("parseSelection", () => {
  function parseSelection(input: string, available: string[]): string[] {
    const trimmed = input.trim();
    if (trimmed === "") return [];

    const indices = trimmed
      .split(/[,，]/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0)
      .map((s) => {
        const n = Number(s);
        return Number.isInteger(n) && n >= 1 && n <= available.length
          ? n - 1
          : -1;
      })
      .filter((i) => i >= 0);

    const unique = [...new Set(indices)];
    return unique.map((i) => available[i]!);
  }

  it('"1" 选中第一个', () => {
    expect(parseSelection("1", ["claude", "codex", "opencode"])).toEqual([
      "claude",
    ]);
  });

  it('"1,3" 选中第一个和第三个', () => {
    expect(parseSelection("1,3", ["claude", "codex", "opencode"])).toEqual([
      "claude",
      "opencode",
    ]);
  });

  it('"1, 3" 带空格正常解析', () => {
    expect(parseSelection("1, 3", ["claude", "codex", "opencode"])).toEqual([
      "claude",
      "opencode",
    ]);
  });

  it("空输入返回空数组", () => {
    expect(parseSelection("", ["claude", "codex"])).toEqual([]);
  });

  it("无效编号被忽略返回空数组", () => {
    expect(parseSelection("5", ["claude", "codex"])).toEqual([]);
  });

  it("去重：输入 1,1 只返回一个", () => {
    expect(parseSelection("1,1", ["claude", "codex"])).toEqual(["claude"]);
  });
});
