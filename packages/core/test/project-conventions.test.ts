import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import {
  createEmptyProjectProfile,
  discoverProjectConventions,
} from "../src/project-conventions/scanner.js";
import { ProjectConventionsStore } from "../src/project-conventions/store.js";

const roots: string[] = [];

async function temporaryRoot(prefix: string): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), prefix));
  roots.push(root);
  return root;
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("project conventions", () => {
  it("discovers an existing project structure with evidence paths", async () => {
    const root = await temporaryRoot("sdd-conventions-");
    await writeFile(
      join(root, "package.json"),
      '{"name":"fixture","workspaces":["packages/*"]}\n',
      "utf8",
    );
    await writeFile(join(root, "tsconfig.json"), "{}\n", "utf8");
    await mkdir(join(root, "src"), { recursive: true });
    await mkdir(join(root, "test"), { recursive: true });
    await writeFile(join(root, "src/index.ts"), "export const demo = 1;\n", {
      encoding: "utf8",
    });
    await writeFile(join(root, "test/index.test.ts"), "export {};\n", {
      encoding: "utf8",
    });
    await writeFile(join(root, "AGENTS.md"), "# rules\n", "utf8");

    const profile = await discoverProjectConventions(root);

    expect(profile).toMatchObject({
      schemaVersion: "1.2.0",
      projectType: "existing",
      strategy: "discovered",
      directories: {
        source: ["src"],
        test: ["test"],
      },
    });
    expect(profile.conventions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: "workspace",
          value: "package.json#workspaces",
          evidence: ["package.json"],
        }),
      ]),
    );
    expect(profile.ruleFiles).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          path: "AGENTS.md",
          scope: ".",
        }),
      ]),
    );
  });

  it("keeps a user-defined profile on repeated writes", async () => {
    const root = await temporaryRoot("sdd-conventions-store-");
    const store = new ProjectConventionsStore(root);
    const userDefined = createEmptyProjectProfile(root, "user-defined");
    const discovered = createEmptyProjectProfile(root, "free-design");

    await store.write({ ...userDefined, strategy: "user-defined" });
    await store.write({ ...discovered, strategy: "discovered" });

    expect(await store.read()).toMatchObject({
      strategy: "user-defined",
      projectType: "empty",
    });
    expect(
      await readFile(join(root, ".sdd/project/conventions.md"), "utf8"),
    ).toContain("user-defined");
  });
});
