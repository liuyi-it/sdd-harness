import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
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
}

export class GitInspector {
  constructor(private readonly root: string) {}

  async snapshot(): Promise<GitSnapshot> {
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
      return { available: true, files, hashes };
    } catch {
      return { available: false, files: [], hashes: {} };
    }
  }

  delta(before: GitSnapshot, after: GitSnapshot): string[] {
    if (!before.available || !after.available) return [];
    return after.files.filter(
      (file) =>
        before.hashes[file] === undefined ||
        before.hashes[file] !== after.hashes[file],
    );
  }
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
