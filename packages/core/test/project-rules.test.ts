import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { resolveProjectRules } from "../src/project-conventions/rule-resolver.js";

const roots: string[] = [];

async function temporaryRoot(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "sdd-rules-"));
  roots.push(root);
  return root;
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("project rules", () => {
  it("prefers AGENTS.md for codex and includes CLAUDE.md as supplement", async () => {
    const root = await temporaryRoot();
    await mkdir(join(root, "src"), { recursive: true });
    await writeFile(join(root, "AGENTS.md"), "# codex\n", "utf8");
    await writeFile(join(root, "CLAUDE.md"), "# claude\n", "utf8");

    const snapshot = await resolveProjectRules(root, ["src/order.ts"], "codex");

    expect(snapshot.host).toBe("codex");
    expect(snapshot.sources.map((entry) => entry.path)).toEqual([
      "AGENTS.md",
      "CLAUDE.md",
    ]);
    expect(snapshot.acknowledgement).toBe("MUST_FOLLOW_PROJECT_RULES");
    expect(snapshot.hash).toMatch(/^sha256:[a-f0-9]{64}$/);
  });

  it("includes nested rule files for scoped task paths", async () => {
    const root = await temporaryRoot();
    await mkdir(join(root, "src", "feature"), { recursive: true });
    await writeFile(join(root, "AGENTS.md"), "# root\n", "utf8");
    await writeFile(
      join(root, "src", "feature", "AGENTS.md"),
      "# nested\n",
      "utf8",
    );

    const snapshot = await resolveProjectRules(
      root,
      ["src/feature/order.ts"],
      "codex",
    );

    expect(snapshot.sources.map((entry) => entry.path)).toEqual([
      "AGENTS.md",
      "src/feature/AGENTS.md",
    ]);
    expect(snapshot.sources.map((entry) => entry.scope)).toEqual([
      ".",
      "src/feature",
    ]);
  });
});
