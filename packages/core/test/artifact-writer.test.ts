import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
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

  it("repairs only missing primary files when existing group inputs match", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-writer-same-"));
    const writer = new ArtifactWriter();
    const paths = ["spec.md", "spec.delta.md", "spec.model.json"].map((name) =>
      join(root, name),
    );
    await writer.write(paths[0]!, "old", { version: 1 });
    await writer.write(paths[1]!, "old", { version: 1 });

    await expect(
      writer.writeGroupOrCandidates(
        paths.map((path) => ({ path, content: "old" })),
        { version: 1 },
      ),
    ).resolves.toBe("written");
    expect(await readFile(paths[2]!, "utf8")).toBe("old\n");
  });

  it.each(["missing+changed", "all-changed"])(
    "writes a complete candidate group for %s inputs",
    async (mode) => {
      const root = await mkdtemp(join(tmpdir(), "sdd-writer-changed-"));
      const writer = new ArtifactWriter();
      const paths = ["spec.md", "spec.delta.md", "spec.model.json"].map(
        (name) => join(root, name),
      );
      const existing = mode === "missing+changed" ? paths.slice(0, 2) : paths;
      for (const path of existing)
        await writer.write(path, "old", { version: 1 });

      await expect(
        writer.writeGroupOrCandidates(
          paths.map((path) => ({ path, content: "new" })),
          { version: 2 },
        ),
      ).resolves.toBe("candidate");
      if (mode === "missing+changed")
        await expect(readFile(paths[2]!, "utf8")).rejects.toThrow();
      for (const path of paths)
        expect(await readFile(`${path}.candidate.md`, "utf8")).toBe("new\n");
    },
  );

  it.each(["missing-meta", "invalid-json", "missing-fields"])(
    "protects existing primary files when metadata is %s",
    async (mode) => {
      const root = await mkdtemp(join(tmpdir(), "sdd-writer-meta-"));
      const writer = new ArtifactWriter();
      const paths = ["spec.md", "spec.delta.md", "spec.model.json"].map(
        (name) => join(root, name),
      );
      for (const path of paths) await writer.write(path, "old", { version: 1 });
      await writeFile(paths[0]!, "manual\n", "utf8");
      const metaPath = `${paths[0]}.meta.json`;
      if (mode === "missing-meta") await rm(metaPath);
      else if (mode === "invalid-json")
        await writeFile(metaPath, "{bad", "utf8");
      else await writeFile(metaPath, '{"schemaVersion":"1.0.0"}\n', "utf8");

      await expect(
        writer.writeGroupOrCandidates(
          paths.map((path) => ({ path, content: "new" })),
          { version: 1 },
        ),
      ).resolves.toBe("candidate");
      expect(await readFile(paths[0]!, "utf8")).toBe("manual\n");
      for (const path of paths)
        expect(await readFile(`${path}.candidate.md`, "utf8")).toBe("new\n");
    },
  );
});
