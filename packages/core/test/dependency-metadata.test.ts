import { access, readdir, readFile } from "node:fs/promises";
import { join, relative, resolve, sep } from "node:path";
import { promisify } from "node:util";
import { execFile } from "node:child_process";

import { describe, expect, it } from "vitest";

import { PINNED_DEPENDENCIES } from "../src/dependencies.js";

const execFileAsync = promisify(execFile);

describe("第三方依赖元数据", () => {
  it("固定依赖版本与第三方声明、vendor 许可证保持一致", async () => {
    const notices = await readFile(
      join(process.cwd(), "THIRD_PARTY_NOTICES.md"),
      "utf8",
    );

    for (const dependency of [
      PINNED_DEPENDENCIES.codebaseMemoryMcp,
      PINNED_DEPENDENCIES.openSpec,
      PINNED_DEPENDENCIES.superpowers,
    ]) {
      expect(notices).toContain(dependency.name);
      expect(notices).toContain(dependency.version);
      expect(notices).toContain(dependency.commit);
      expect(notices).toContain(dependency.license);
    }

    for (const path of [
      "vendor/openspec/upstream/LICENSE",
      "vendor/superpowers/upstream/LICENSE",
    ]) {
      await expect(access(join(process.cwd(), path))).resolves.toBeUndefined();
      expect(await readFile(join(process.cwd(), path), "utf8")).toContain(
        "MIT License",
      );
    }
  });

  it("固定完整上游快照的来源、版本与本地修改声明", async () => {
    const expected = {
      openspec: {
        ...pickVersionMetadata(PINNED_DEPENDENCIES.openSpec),
        localModifications: "None; adapters live outside upstream/.",
      },
      superpowers: {
        ...pickVersionMetadata(PINNED_DEPENDENCIES.superpowers),
        localModifications: "None; adapters live outside upstream/.",
      },
    };

    for (const [directory, metadata] of Object.entries(expected)) {
      const vendorRoot = join(process.cwd(), "vendor", directory);
      expect(
        JSON.parse(await readFile(join(vendorRoot, "VERSION.json"), "utf8")),
      ).toEqual(metadata);
      expect(
        await readFile(join(vendorRoot, "MANIFEST.sha256"), "utf8"),
      ).toMatch(/^[a-f0-9]{64} {2}file upstream\/.+$/m);
      expect(
        await readFile(join(vendorRoot, "upstream/LICENSE"), "utf8"),
      ).toContain("MIT License");
    }
  });

  it("上游快照清单可重现并完整覆盖文件与符号链接", async () => {
    const manifestModule = (await import(
      "../../../scripts/vendor-manifest.mjs"
    )) as Record<string, unknown>;
    expect(manifestModule.computeVendorManifest).toBeTypeOf("function");
    const computeVendorManifest = manifestModule.computeVendorManifest as (
      vendorRoot: string,
    ) => Promise<string>;

    for (const directory of ["openspec", "superpowers"]) {
      const vendorRoot = join(process.cwd(), "vendor", directory);
      const committed = await readFile(
        join(vendorRoot, "MANIFEST.sha256"),
        "utf8",
      );
      expect(await computeVendorManifest(vendorRoot)).toBe(committed);

      const lines = committed.trimEnd().split("\n");
      const manifestPaths = lines.map((line) => {
        const match =
          /^[a-f0-9]{64} {2}(?:file|symlink) upstream\/(.+?)(?: -> .+)?$/.exec(
            line,
          );
        expect(match).not.toBeNull();
        return match?.[1];
      });
      expect(manifestPaths).toEqual([...manifestPaths].sort());
      expect(manifestPaths).toEqual(await listSnapshotEntries(vendorRoot));
    }

    expect(
      await readFile(
        join(process.cwd(), "vendor/superpowers/MANIFEST.sha256"),
        "utf8",
      ),
    ).toMatch(
      /^[a-f0-9]{64} {2}symlink upstream\/AGENTS\.md -> "CLAUDE\.md"$/m,
    );
  });

  it("仅对原样上游快照关闭 Git 空白错误检查", async () => {
    expect(await readFile(join(process.cwd(), ".gitattributes"), "utf8")).toBe(
      "vendor/openspec/upstream/** -whitespace\n" +
        "vendor/superpowers/upstream/** -whitespace\n",
    );
  });

  it("公共声明保留固定依赖的 readonly 与版本字面量类型", async () => {
    await execFileAsync(
      process.execPath,
      [
        "node_modules/typescript/bin/tsc",
        "-b",
        "packages/core",
        "--pretty",
        "false",
      ],
      { cwd: process.cwd() },
    );
    const declaration = await readFile(
      join(process.cwd(), "packages/core/dist/pinned-dependencies.d.mts"),
      "utf8",
    );

    expect(declaration).toContain('readonly version: "v1.4.1"');
    expect(declaration).toContain("readonly openSpec:");
    expect(declaration).not.toMatch(/version: string/);
  });
});

function pickVersionMetadata(dependency: {
  name: string;
  version: string;
  commit: string;
  repository: string;
  license: string;
}) {
  const { name, version, commit, repository, license } = dependency;
  return { name, version, commit, repository, license };
}

async function listSnapshotEntries(vendorRoot: string) {
  const upstreamRoot = resolve(vendorRoot, "upstream");
  const entries: string[] = [];

  async function visit(directory: string) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = resolve(directory, entry.name);
      if (entry.isDirectory()) await visit(path);
      else entries.push(relative(upstreamRoot, path).split(sep).join("/"));
    }
  }

  await visit(upstreamRoot);
  return entries.sort();
}

describe("仓库元数据与安装文档", () => {
  it("提供需求文档要求的必备文档", async () => {
    for (const path of [
      "README.md",
      "THIRD_PARTY_NOTICES.md",
      "docs/architecture.md",
      "docs/state-machine.md",
      "docs/security.md",
      "docs/plugin-installation.md",
      "docs/command-contract.md",
      "docs/schemas.md",
    ]) {
      await expect(access(join(process.cwd(), path))).resolves.toBeUndefined();
    }
  });

  it("声明 Node.js 20+ 引擎，并在文档中约束平台与手工卸载路径", async () => {
    const packageJson = JSON.parse(
      await readFile(join(process.cwd(), "package.json"), "utf8"),
    ) as {
      engines?: { node?: string };
    };
    const [readme, installDoc] = await Promise.all([
      readFile(join(process.cwd(), "README.md"), "utf8"),
      readFile(join(process.cwd(), "docs/plugin-installation.md"), "utf8"),
    ]);

    expect(packageJson.engines?.node).toBe(">=20");

    for (const document of [readme, installDoc]) {
      expect(document).toContain("macOS");
      expect(document).toContain("Windows");
      expect(document).toContain(".sdd/");
      expect(document).toContain(".claude/commands/sdd.*");
      expect(document).toContain(".claude/skills/sdd-harness/");
      expect(document).toContain(".codex/skills/sdd-harness/");
    }
  });

  it("README 提供 Claude Code 与 Codex 的一键安装脚本说明", async () => {
    const readme = await readFile(join(process.cwd(), "README.md"), "utf8");

    expect(readme).toContain("node scripts/install-claude.mjs");
    expect(readme).toContain("node scripts/install-codex.mjs");
    expect(readme).toContain("最后一步宿主内安装/重载");
    expect(readme).toContain("~/.agents/plugins/marketplace.json");
  });

  it("提供需求文档要求的示例 fixture 目录", async () => {
    for (const path of [
      "fixtures/springboot-order-service",
      "fixtures/node-basic-service",
      "fixtures/broken-state-project",
      "fixtures/security-malicious-readme",
    ]) {
      await expect(access(join(process.cwd(), path))).resolves.toBeUndefined();
    }
  });
});
