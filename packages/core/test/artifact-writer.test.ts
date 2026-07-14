import { mkdir, mkdtemp, readFile, readdir, rename } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { ArtifactWriter } from "../src/artifacts/artifact-writer.js";

describe("ArtifactWriter.writeGroupAtomically", () => {
  (process.platform === "win32" ? it.skip.each([3, 4, 5]) : it.each([3, 4, 5]))(
    "组写发布阶段第 %i 次 rename 失败时完整恢复已有主制品",
    async (failedRename) => {
      const root = await mkdtemp(join(tmpdir(), "sdd-writer-rollback-"));
      await mkdir(join(root, ".sdd"));
      const paths = [join(root, "traceability.md"), join(root, "archive.md")];
      const seed = new ArtifactWriter();
      for (const path of paths)
        await seed.write(path, `old:${path}`, { old: true });
      const originals = new Map<string, string>();
      for (const path of [...paths, join(root, ".sdd", "artifacts.json")])
        originals.set(path, await readFile(path, "utf8"));

      let renameCount = 0;
      const writer = new ArtifactWriter(async (source, target) => {
        renameCount += 1;
        if (renameCount === failedRename) throw new Error("注入 rename 失败");
        await rename(source, target);
      });
      await expect(
        writer.writeGroupAtomically(
          paths.map((path) => ({
            path,
            content: "new",
            inputs: { new: true },
          })),
        ),
      ).rejects.toThrow("注入 rename 失败");

      for (const [path, content] of originals)
        expect(await readFile(path, "utf8")).toBe(content);
      expect(
        (await readdir(root)).filter(
          (name) => name.includes(".tmp-") || name.includes(".bak-"),
        ),
      ).toEqual([]);
    },
  );
});
