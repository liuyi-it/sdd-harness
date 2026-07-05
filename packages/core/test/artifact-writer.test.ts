import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { ArtifactWriter } from "../src/artifacts/artifact-writer.js";

describe("ArtifactWriter.writeOrCandidate", () => {
  it("keeps unchanged files idempotent and protects changed inputs with candidates", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-writer-"));
    const path = join(root, "spec.md");
    const writer = new ArtifactWriter();
    await writer.write(path, "one", { version: 1 });

    await expect(
      writer.writeOrCandidate(path, "one", { version: 1 }),
    ).resolves.toBe("unchanged");
    await expect(
      writer.writeOrCandidate(path, "two", { version: 2 }),
    ).resolves.toBe("candidate");
    expect(await readFile(path, "utf8")).toBe("one\n");

    await writeFile(path, "manual\n", "utf8");
    await expect(
      writer.writeOrCandidate(path, "three", { version: 3 }),
    ).resolves.toBe("candidate");
    expect(await readFile(path, "utf8")).toBe("manual\n");
    expect(await readFile(`${path}.candidate.md`, "utf8")).toBe("three\n");
  });

  it("keeps a related artifact group consistent when one file was edited", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-writer-group-"));
    const writer = new ArtifactWriter();
    const paths = ["spec.md", "spec.delta.md", "spec.model.json"].map((name) =>
      join(root, name),
    );
    for (const path of paths) await writer.write(path, "old", { version: 1 });
    await writeFile(paths[0]!, "manual\n", "utf8");

    await expect(
      writer.writeGroupOrCandidates(
        paths.map((path) => ({ path, content: "new" })),
        { version: 2 },
      ),
    ).resolves.toBe("candidate");
    expect(await readFile(paths[1]!, "utf8")).toBe("old\n");
    expect(await readFile(paths[2]!, "utf8")).toBe("old\n");
    for (const path of paths)
      expect(await readFile(`${path}.candidate.md`, "utf8")).toBe("new\n");
  });
});
