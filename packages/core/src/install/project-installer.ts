import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { COMMANDS } from "../contracts.js";
import { CANONICAL_SCHEMAS } from "./canonical-schemas.js";

const MANAGED_MARKER = "<!-- sdd-harness:managed -->";
const KARPATHY_RULES = `Karpathy-inspired operating rules:
1. Think Before Coding — state assumptions, surface ambiguity and tradeoffs, ask instead of guessing.
2. Simplicity First — write the minimum code that solves the requested problem; avoid speculative abstractions.
3. Surgical Changes — touch only files and lines required by the task; do not refactor unrelated code.
4. Goal-Driven Execution — define concrete verification steps, prefer tests or checks first, and do not claim success before verification.`;

export async function installProjectIntegration(root: string): Promise<void> {
  await Promise.all([
    installInstructions(join(root, "CLAUDE.md"), claudeInstructions()),
    installInstructions(join(root, "AGENTS.md"), codexInstructions()),
    installClaudeCommands(root),
    installSkills(root),
    installSchemas(root),
  ]);
}

async function installInstructions(
  path: string,
  managedContent: string,
): Promise<void> {
  let existing = "";
  try {
    existing = await readFile(path, "utf8");
  } catch {
    // A missing instruction file is expected on first init.
  }
  if (existing.includes(MANAGED_MARKER)) return;
  const separator = existing === "" || existing.endsWith("\n") ? "" : "\n";
  await writeFile(path, `${existing}${separator}${managedContent}\n`, "utf8");
}

async function installClaudeCommands(root: string): Promise<void> {
  const directory = join(root, ".claude", "commands");
  await mkdir(directory, { recursive: true });
  await Promise.all(
    COMMANDS.map((command) =>
      writeIfMissing(
        join(directory, `sdd.${command}.md`),
        `---\ndescription: Execute sdd ${command} through sdd-harness\n---\n\nUse the installed ClaudeCodeAdapter to execute /sdd.${command} $ARGUMENTS. Return the Core CommandResult without bypassing gates.\n\n${KARPATHY_RULES}\n`,
      ),
    ),
  );
}

async function installSkills(root: string): Promise<void> {
  const content = `---
name: sdd-harness
description: Use when a repository change must follow the sdd init, new, design, plan, build, verify, review, archive, auto, or status workflow.
---

# SDD Harness

Execute requests through the installed platform Adapter. Treat .sdd/ as the only workflow fact source. Do not bypass phase, lock, file-scope, verification, review, or archive gates. Stop on CLARIFYING, FAILED, or PAUSED.

${KARPATHY_RULES}
`;
  for (const path of [
    join(root, ".claude", "skills", "sdd-harness", "SKILL.md"),
    join(root, ".codex", "skills", "sdd-harness", "SKILL.md"),
  ]) {
    await mkdir(join(path, ".."), { recursive: true });
    await writeIfMissing(path, content);
  }
}

async function installSchemas(root: string): Promise<void> {
  const directory = join(root, ".sdd", "schemas");
  await mkdir(directory, { recursive: true });
  await Promise.all(
    Object.entries(CANONICAL_SCHEMAS).map(async ([name, content]) =>
      writeIfMissing(join(directory, name), content),
    ),
  );
}

function claudeInstructions(): string {
  return `${MANAGED_MARKER}
## sdd-harness

Use /sdd.auto or phase commands for repository changes. .sdd/ is the only workflow fact source. Build must read the task Context Pack and stay inside Allowed Files. Stop when verify or review fails.

${KARPATHY_RULES}`;
}

function codexInstructions(): string {
  return `${MANAGED_MARKER}
## sdd-harness

Use sdd auto or phase commands for repository changes. .sdd/ is the only workflow fact source. Build must read the task Context Pack and stay inside Allowed Files. Stop when verify or review fails.

${KARPATHY_RULES}`;
}

async function writeIfMissing(path: string, content: string): Promise<void> {
  try {
    await access(path);
  } catch {
    await writeFile(path, content, "utf8");
  }
}
