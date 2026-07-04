import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

const SOURCE_FILES = [
  "packages/core/src/commands/archive.ts",
  "packages/core/src/commands/build.ts",
  "packages/core/src/commands/design.ts",
  "packages/core/src/commands/init.ts",
  "packages/core/src/commands/new.ts",
  "packages/core/src/commands/plan.ts",
  "packages/core/src/commands/review.ts",
  "packages/core/src/commands/verify.ts",
  "packages/core/src/install/project-installer.ts",
  "packages/core/src/state/state-store.ts",
] as const;

describe("制品写入回归保护", () => {
  it("核心命令不直接 writeFile 生成 Markdown 文档", async () => {
    const markdownWritePattern =
      /writeFile\([\s\S]{0,200}?["'`](?:[^"'`]*\.md(?:\.candidate\.md)?)["'`]/g;

    for (const relativePath of SOURCE_FILES) {
      const source = await readFile(join(process.cwd(), relativePath), "utf8");
      expect(source.match(markdownWritePattern), relativePath).toBeNull();
    }
  });
});
