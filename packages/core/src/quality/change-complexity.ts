import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { promisify } from "node:util";

import type { GitSnapshot } from "../git/git-inspector.js";

const executeFile = promisify(execFile);

export interface ChangeComplexityMetrics {
  filesAdded: number;
  filesModified: number;
  filesDeleted: number;
  linesAdded: number | null;
  linesDeleted: number | null;
  netLines: number | null;
  dependenciesAdded: number;
  dependenciesRemoved: number;
  deliberateDebtCount: number;
}

export function changedFiles(
  baseline: GitSnapshot,
  current: GitSnapshot,
): string[] {
  if (!baseline.available || !current.available) return [];
  return [...new Set([...baseline.files, ...current.files])]
    .filter((file) => baseline.hashes[file] !== current.hashes[file])
    .sort();
}

export function fileMetrics(
  baseline: GitSnapshot,
  current: GitSnapshot,
): Pick<
  ChangeComplexityMetrics,
  "filesAdded" | "filesModified" | "filesDeleted"
> {
  let filesAdded = 0;
  let filesModified = 0;
  let filesDeleted = 0;
  for (const file of changedFiles(baseline, current)) {
    if (current.hashes[file] === "deleted") filesDeleted += 1;
    else if (!baseline.tracked.includes(file)) filesAdded += 1;
    else filesModified += 1;
  }
  return { filesAdded, filesModified, filesDeleted };
}

export async function collectChangeComplexityMetrics(
  root: string,
  baseline: GitSnapshot,
  current: GitSnapshot,
): Promise<
  Pick<
    ChangeComplexityMetrics,
    | "filesAdded"
    | "filesModified"
    | "filesDeleted"
    | "linesAdded"
    | "linesDeleted"
    | "netLines"
  >
> {
  const files = changedFiles(baseline, current);
  const metrics = fileMetrics(baseline, current);
  if (!baseline.available || !current.available)
    return { ...metrics, linesAdded: null, linesDeleted: null, netLines: null };
  if (files.length === 0)
    return { ...metrics, linesAdded: 0, linesDeleted: 0, netLines: 0 };
  try {
    const { stdout } = await executeFile(
      "git",
      ["diff", "--numstat", "--", ...files],
      {
        cwd: root,
        encoding: "utf8",
      },
    );
    const parsed = parseNumstat(stdout);
    const untracked = files.filter((file) => !baseline.tracked.includes(file));
    const additions = await Promise.all(
      untracked.map((file) => lineCount(join(root, file))),
    );
    if (parsed === null || additions.some((count) => count === null))
      return {
        ...metrics,
        linesAdded: null,
        linesDeleted: null,
        netLines: null,
      };
    const linesAdded =
      parsed.added +
      additions.reduce<number>((total, count) => total + (count ?? 0), 0);
    const linesDeleted = parsed.deleted;
    return {
      ...metrics,
      linesAdded,
      linesDeleted,
      netLines: linesAdded - linesDeleted,
    };
  } catch {
    return { ...metrics, linesAdded: null, linesDeleted: null, netLines: null };
  }
}

function parseNumstat(
  output: string,
): { added: number; deleted: number } | null {
  let added = 0;
  let deleted = 0;
  for (const line of output.split("\n").filter(Boolean)) {
    const [rawAdded, rawDeleted] = line.split("\t", 3);
    if (
      rawAdded === "-" ||
      rawDeleted === "-" ||
      rawAdded === undefined ||
      rawDeleted === undefined
    )
      return null;
    const parsedAdded = Number(rawAdded);
    const parsedDeleted = Number(rawDeleted);
    if (!Number.isInteger(parsedAdded) || !Number.isInteger(parsedDeleted))
      return null;
    added += parsedAdded;
    deleted += parsedDeleted;
  }
  return { added, deleted };
}

async function lineCount(path: string): Promise<number | null> {
  try {
    const content = await readFile(path);
    if (content.includes(0)) return null;
    if (content.length === 0) return 0;
    const text = content.toString("utf8");
    return text.split("\n").length - (text.endsWith("\n") ? 1 : 0);
  } catch {
    return null;
  }
}
