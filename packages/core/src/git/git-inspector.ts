import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { access, readFile } from "node:fs/promises";
import { join } from "node:path";
import { promisify } from "node:util";

/**
 * GitInspector 用来记录任务执行前后的仓库快照，并计算真实差异。
 * 这样 build 阶段可以用 Git 证据补全或纠偏执行器返回的 modifiedFiles。
 */
const executeFile = promisify(execFile);

export interface GitSnapshot {
  available: boolean;
  files: string[];
  hashes: Record<string, string>;
  tracked: string[];
  /** 仅保留 package.json 快照，以便 review 在不读取历史工作区的情况下比较依赖变化。 */
  manifests?: Record<string, string | null>;
}

export class GitInspector {
  constructor(private readonly root: string) {}

  async snapshot(): Promise<GitSnapshot> {
    if (!(await isGitRepository(this.root))) {
      return { available: false, files: [], hashes: {}, tracked: [] };
    }
    try {
      const { stdout } = await executeFile(
        "git",
        ["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        { cwd: this.root, encoding: "utf8" },
      );
      const files = parsePorcelain(stdout)
        .filter((file) => !isInternal(file))
        .sort();
      const hashes = Object.fromEntries(
        await Promise.all(
          files.map(
            async (file) =>
              [file, await fileHash(join(this.root, file))] as const,
          ),
        ),
      );
      const { stdout: trackedOutput } = await executeFile(
        "git",
        ["ls-files", "-z"],
        { cwd: this.root, encoding: "utf8" },
      );
      const tracked = trackedOutput
        .split("\0")
        .filter(Boolean)
        .map((file) => file.replaceAll("\\", "/"))
        .filter((file) => !isInternal(file))
        .sort();
      const manifests = await snapshotManifests(this.root, [
        ...new Set([...tracked, ...files]),
      ]);
      return { available: true, files, hashes, tracked, manifests };
    } catch {
      return { available: false, files: [], hashes: {}, tracked: [] };
    }
  }

  delta(before: GitSnapshot, after: GitSnapshot): string[] {
    if (!before.available || !after.available) return [];
    return [...new Set([...before.files, ...after.files])].filter(
      (file) =>
        before.hashes[file] !== after.hashes[file] ||
        before.files.includes(file) !== after.files.includes(file),
    );
  }
}

export function snapshotFromJson(input: unknown): GitSnapshot | null {
  if (
    typeof input !== "object" ||
    input === null ||
    !("available" in input) ||
    !("files" in input) ||
    !("hashes" in input)
  ) {
    return null;
  }
  const candidate = input as {
    available?: unknown;
    files?: unknown;
    hashes?: unknown;
    tracked?: unknown;
    manifests?: unknown;
  };
  if (
    typeof candidate.available !== "boolean" ||
    !Array.isArray(candidate.files) ||
    typeof candidate.hashes !== "object" ||
    candidate.hashes === null
  ) {
    return null;
  }
  if (
    !candidate.files.every((file) => typeof file === "string") ||
    !Object.values(candidate.hashes).every((hash) => typeof hash === "string")
  ) {
    return null;
  }
  return {
    available: candidate.available,
    files: candidate.files,
    hashes: candidate.hashes as Record<string, string>,
    tracked: Array.isArray(candidate.tracked)
      ? candidate.tracked.filter(
          (file): file is string => typeof file === "string",
        )
      : [],
    ...(isManifestSnapshot(candidate.manifests)
      ? { manifests: candidate.manifests }
      : {}),
  };
}

async function snapshotManifests(
  root: string,
  files: readonly string[],
): Promise<Record<string, string | null>> {
  const manifests = files.filter(
    (file) => file === "package.json" || file.endsWith("/package.json"),
  );
  const entries = await Promise.all(
    manifests.map(async (file) => {
      try {
        return [file, await readFile(join(root, file), "utf8")] as const;
      } catch {
        return [file, null] as const;
      }
    }),
  );
  return Object.fromEntries(entries);
}

function isManifestSnapshot(
  value: unknown,
): value is Record<string, string | null> {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    Object.entries(value).every(
      ([key, content]) =>
        (key === "package.json" || key.endsWith("/package.json")) &&
        (typeof content === "string" || content === null),
    )
  );
}

function parsePorcelain(output: string): string[] {
  const records = output.split("\0").filter(Boolean);
  const files: string[] = [];
  for (let index = 0; index < records.length; index += 1) {
    const record = records[index];
    if (record === undefined || record.length < 4) continue;
    const status = record.slice(0, 2);
    files.push(record.slice(3).replaceAll("\\", "/"));
    if (status.includes("R") || status.includes("C")) index += 1;
  }
  return files;
}

function isInternal(file: string): boolean {
  return file === ".sdd" || file.startsWith(".sdd/");
}

async function fileHash(path: string): Promise<string> {
  try {
    return createHash("sha256")
      .update(await readFile(path))
      .digest("hex");
  } catch {
    return "deleted";
  }
}

async function isGitRepository(root: string): Promise<boolean> {
  try {
    await access(join(root, ".git"));
    return true;
  } catch {
    return false;
  }
}
