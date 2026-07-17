import { execFile } from "node:child_process";
import {
  access,
  chmod,
  cp,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";

const execFileAsync = promisify(execFile);
const repositoryRoot = fileURLToPath(new URL("../../..", import.meta.url));
const temporaryRoots: string[] = [];

async function exists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

async function makeExecutable(path: string, contents: string): Promise<void> {
  await writeFile(path, contents, "utf8");
  await chmod(path, 0o755);
}

async function createFixture(): Promise<{
  root: string;
  env: NodeJS.ProcessEnv;
  log: string;
  globalRoot: string;
  globalPrefix: string;
}> {
  const root = await mkdtemp(join(tmpdir(), "sdd-installation-"));
  temporaryRoots.push(root);

  await mkdir(join(root, "scripts", "lib"), { recursive: true });
  await mkdir(join(root, "packages", "cli", "dist"), { recursive: true });
  await mkdir(join(root, "packages", "core", "node_modules"), {
    recursive: true,
  });
  await mkdir(join(root, "packages", "core", "dist"), { recursive: true });
  await mkdir(join(root, "node_modules"), { recursive: true });
  await mkdir(join(root, ".sdd"), { recursive: true });
  await cp(
    join(repositoryRoot, "scripts", "install.sh"),
    join(root, "scripts", "install.sh"),
  );
  await cp(
    join(repositoryRoot, "scripts", "uninstall.sh"),
    join(root, "scripts", "uninstall.sh"),
  );
  await cp(
    join(repositoryRoot, "scripts", "lib", "installation.sh"),
    join(root, "scripts", "lib", "installation.sh"),
  );

  await writeFile(join(root, "node_modules", "stale"), "旧依赖", "utf8");
  await writeFile(
    join(root, "packages", "cli", "dist", "stale.js"),
    "旧构建",
    "utf8",
  );
  await writeFile(
    join(root, "packages", "core", "dist", "stale.js"),
    "旧构建",
    "utf8",
  );
  await writeFile(
    join(root, "packages", "core", "node_modules", "stale"),
    "旧依赖",
    "utf8",
  );
  await writeFile(
    join(root, "packages", "core", "tsconfig.tsbuildinfo"),
    "旧缓存",
    "utf8",
  );
  await writeFile(join(root, ".sdd", "state.json"), "用户数据", "utf8");

  const bin = join(root, "test-bin");
  const globalRoot = join(root, "global", "lib", "node_modules");
  const globalPrefix = join(root, "global");
  const log = join(root, "npm.log");
  await mkdir(bin, { recursive: true });
  await mkdir(join(globalRoot, "@sdd-harness", "cli"), { recursive: true });
  await mkdir(join(globalPrefix, "bin"), { recursive: true });

  await makeExecutable(
    join(bin, "node"),
    "#!/usr/bin/env bash\necho v22.0.0\n",
  );
  await makeExecutable(
    join(bin, "npm"),
    `#!/usr/bin/env bash
set -euo pipefail
echo "$*" >> "$SDD_TEST_LOG"
case "$*" in
  "root --global") echo "$SDD_TEST_GLOBAL_ROOT" ;;
  "prefix --global") echo "$SDD_TEST_GLOBAL_PREFIX" ;;
  "ci") mkdir -p "$SDD_TEST_ROOT/node_modules"; echo fresh > "$SDD_TEST_ROOT/node_modules/fresh" ;;
  "run build")
    mkdir -p "$SDD_TEST_ROOT/packages/cli/dist"
    echo fresh > "$SDD_TEST_ROOT/packages/cli/dist/cli.js"
    if [ "\${SDD_TEST_FAIL_BUILD:-}" = true ]; then exit 23; fi
    ;;
  "link --workspace=packages/cli") ;;
esac
`,
  );
  await makeExecutable(join(bin, "sdd"), "#!/usr/bin/env bash\necho 0.1.0\n");
  await makeExecutable(
    join(bin, "sdd-harness"),
    "#!/usr/bin/env bash\necho 0.1.0\n",
  );

  return {
    root,
    log,
    globalRoot,
    globalPrefix,
    env: {
      ...process.env,
      PATH: `${bin}:${process.env.PATH ?? ""}`,
      SDD_TEST_ROOT: root,
      SDD_TEST_LOG: log,
      SDD_TEST_GLOBAL_ROOT: globalRoot,
      SDD_TEST_GLOBAL_PREFIX: globalPrefix,
    },
  };
}

afterEach(async () => {
  await Promise.all(
    temporaryRoots
      .splice(0)
      .map((root) => rm(root, { recursive: true, force: true })),
  );
});

describe("安装与卸载脚本", () => {
  it("安装前彻底清理旧依赖、旧构建和全局残留", async () => {
    const fixture = await createFixture();
    await writeFile(
      join(fixture.globalPrefix, "bin", "sdd"),
      `#!/usr/bin/env node\nrequire('${fixture.root}/packages/cli')\n`,
      "utf8",
    );

    await execFileAsync("bash", [join(fixture.root, "scripts", "install.sh")], {
      env: fixture.env,
    });

    expect(await exists(join(fixture.root, "node_modules", "stale"))).toBe(
      false,
    );
    expect(await exists(join(fixture.root, "node_modules", "fresh"))).toBe(
      true,
    );
    expect(
      await exists(join(fixture.root, "packages", "cli", "dist", "stale.js")),
    ).toBe(false);
    expect(
      await exists(join(fixture.root, "packages", "cli", "dist", "cli.js")),
    ).toBe(true);
    expect(
      await exists(join(fixture.root, "packages", "core", "node_modules")),
    ).toBe(false);
    expect(await exists(join(fixture.root, "packages", "core", "dist"))).toBe(
      false,
    );
    expect(
      await exists(
        join(fixture.root, "packages", "core", "tsconfig.tsbuildinfo"),
      ),
    ).toBe(false);
    expect(await exists(join(fixture.globalRoot, "@sdd-harness", "cli"))).toBe(
      false,
    );
    expect(await exists(join(fixture.globalPrefix, "bin", "sdd"))).toBe(false);

    const npmCalls = await readFile(fixture.log, "utf8");
    expect(npmCalls).toContain("uninstall --global @sdd-harness/cli");
    expect(npmCalls.indexOf("ci")).toBeLessThan(npmCalls.indexOf("run build"));
    expect(npmCalls.indexOf("run build")).toBeLessThan(
      npmCalls.indexOf("link --workspace=packages/cli"),
    );
  });

  it("安装失败时清理所有未完成产物", async () => {
    const fixture = await createFixture();

    await expect(
      execFileAsync("bash", [join(fixture.root, "scripts", "install.sh")], {
        env: { ...fixture.env, SDD_TEST_FAIL_BUILD: "true" },
      }),
    ).rejects.toMatchObject({ code: 23 });

    expect(await exists(join(fixture.root, "node_modules"))).toBe(false);
    expect(await exists(join(fixture.root, "packages", "cli", "dist"))).toBe(
      false,
    );
    expect(await exists(join(fixture.root, "packages", "core", "dist"))).toBe(
      false,
    );
    expect(await exists(join(fixture.globalRoot, "@sdd-harness", "cli"))).toBe(
      false,
    );
  });

  it("卸载后不残留安装器拥有的本地文件或全局入口", async () => {
    const fixture = await createFixture();
    await writeFile(
      join(fixture.globalPrefix, "bin", "sdd-harness"),
      "#!/usr/bin/env node\nrequire('@sdd-harness\\cli')\n",
      "utf8",
    );

    await execFileAsync(
      "bash",
      [join(fixture.root, "scripts", "uninstall.sh")],
      {
        env: fixture.env,
      },
    );

    expect(await exists(join(fixture.root, "node_modules"))).toBe(false);
    expect(await exists(join(fixture.root, "packages", "cli", "dist"))).toBe(
      false,
    );
    expect(
      await exists(join(fixture.root, "packages", "core", "node_modules")),
    ).toBe(false);
    expect(await exists(join(fixture.root, "packages", "core", "dist"))).toBe(
      false,
    );
    expect(
      await exists(
        join(fixture.root, "packages", "core", "tsconfig.tsbuildinfo"),
      ),
    ).toBe(false);
    expect(await exists(join(fixture.globalRoot, "@sdd-harness", "cli"))).toBe(
      false,
    );
    expect(await exists(join(fixture.globalPrefix, "bin", "sdd-harness"))).toBe(
      false,
    );
    expect(
      await readFile(join(fixture.root, ".sdd", "state.json"), "utf8"),
    ).toBe("用户数据");
  });
});
