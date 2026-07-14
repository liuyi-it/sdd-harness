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
      PINNED_DEPENDENCIES.mattpocockSkills,
    ]) {
      expect(notices).toContain(dependency.name);
      expect(notices).toContain(dependency.version);
      expect(notices).toContain(dependency.commit);
      expect(notices).toContain(dependency.license);
    }

    for (const path of [
      "vendor/openspec/LICENSE",
      "vendor/openspec/upstream/LICENSE",
      "vendor/superpowers/LICENSE",
      "vendor/superpowers/upstream/LICENSE",
      "vendor/mattpocock-skills/LICENSE",
      "vendor/mattpocock-skills/upstream/LICENSE",
    ]) {
      await expect(access(join(process.cwd(), path))).resolves.toBeUndefined();
      expect(await readFile(join(process.cwd(), path), "utf8")).toContain(
        "MIT License",
      );
    }
  });

  it("固定 mattpocock/skills 来源并保留上游审计记录", async () => {
    const upstream = await readFile(
      join(process.cwd(), "vendor/mattpocock-skills/UPSTREAM.md"),
      "utf8",
    );
    expect(upstream).toContain(PINNED_DEPENDENCIES.mattpocockSkills.repository);
    expect(upstream).toContain(PINNED_DEPENDENCIES.mattpocockSkills.commit);
    expect(upstream).toContain("License: MIT");
    expect(upstream).toContain("Adapted policies:");
  });

  it("固定完整上游快照的来源、版本与本地修改声明", async () => {
    const expected = {
      openspec: {
        ...pickVersionMetadata(PINNED_DEPENDENCIES.openSpec),
        localModifications: "None; adapters live outside upstream/.",
      },
      superpowers: {
        ...pickVersionMetadata(PINNED_DEPENDENCIES.superpowers),
        localModifications:
          "AGENTS.md materialized as a regular copy of CLAUDE.md for Windows checkout compatibility; adapters live outside upstream/.",
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

    const superpowersManifest = await readFile(
      join(process.cwd(), "vendor/superpowers/MANIFEST.sha256"),
      "utf8",
    );
    expect(superpowersManifest).toMatch(
      /^[a-f0-9]{64} {2}file upstream\/AGENTS\.md$/m,
    );
    const agentsDigest = superpowersManifest.match(
      /^([a-f0-9]{64}) {2}file upstream\/AGENTS\.md$/m,
    )?.[1];
    const claudeDigest = superpowersManifest.match(
      /^([a-f0-9]{64}) {2}file upstream\/CLAUDE\.md$/m,
    )?.[1];
    expect(agentsDigest).toBe(claudeDigest);
  });

  it("对所有文件强制 LF 行尾，仅对上游快照关闭空白检查", async () => {
    const content = await readFile(
      join(process.cwd(), ".gitattributes"),
      "utf8",
    );
    expect(content).toContain("* text=auto eol=lf");
    expect(content).toContain("vendor/openspec/upstream/** -whitespace");
    expect(content).toContain("vendor/superpowers/upstream/** -whitespace");
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
    expect(declaration).toContain("readonly mattpocockSkills:");
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
  it("提供当前维护所需的核心文档", async () => {
    for (const path of [
      "README.md",
      "THIRD_PARTY_NOTICES.md",
      "docs/architecture.md",
      "docs/state-machine.md",
      "docs/security.md",
      "docs/CLI.md",
      "docs/command-contract.md",
      "docs/schemas.md",
    ]) {
      await expect(access(join(process.cwd(), path))).resolves.toBeUndefined();
    }
  });

  it("声明 Node.js 22+ 引擎，并在文档中约束平台与安装方式", async () => {
    const packageJson = JSON.parse(
      await readFile(join(process.cwd(), "package.json"), "utf8"),
    ) as {
      engines?: { node?: string };
    };
    const [readme, cliDoc] = await Promise.all([
      readFile(join(process.cwd(), "README.md"), "utf8"),
      readFile(join(process.cwd(), "docs/CLI.md"), "utf8"),
    ]);

    expect(packageJson.engines?.node).toBe(">=22");

    for (const document of [readme, cliDoc]) {
      expect(document).toContain("macOS");
      expect(document).toContain("Windows");
      expect(document).toContain(".sdd/");
    }
  });

  it("README 提供 CLI-first 安装脚本说明", async () => {
    const readme = await readFile(join(process.cwd(), "README.md"), "utf8");

    expect(readme).toContain("bash scripts/install.sh");
    expect(readme).toContain("scripts/install.sh");
    expect(readme).toContain("git clone");
    expect(readme).not.toContain("npm install -g");
  });

  it("提供核心场景使用的示例 fixture 目录", async () => {
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
