import { readFile, stat } from "node:fs/promises";
import { join } from "node:path";

import { changedFiles } from "./change-complexity.js";
import type { GitSnapshot } from "../git/git-inspector.js";
import { createReviewIssue, type ReviewIssue } from "./review-report.js";

const MAX_FILE_SIZE = 1_000_000;
const EXCLUDED_DIRECTORIES = new Set([
  ".git",
  ".sdd",
  "node_modules",
  "dist",
  "build",
  "coverage",
]);

export interface DeliberateDebt {
  file: string;
  line: number;
  ceiling: string;
  trigger: string;
  upgrade: string;
}

export interface DebtScanResult {
  debts: DeliberateDebt[];
  issues: ReviewIssue[];
}

export async function scanDeliberateDebt(
  root: string,
  baseline: GitSnapshot,
  current: GitSnapshot,
): Promise<DebtScanResult> {
  const debts: DeliberateDebt[] = [];
  const issues: ReviewIssue[] = [];
  for (const file of changedFiles(baseline, current)) {
    if (shouldSkip(file) || current.hashes[file] === "deleted") continue;
    const path = join(root, file);
    try {
      if ((await stat(path)).size > MAX_FILE_SIZE) continue;
      const content = await readFile(path);
      if (content.includes(0)) continue;
      content
        .toString("utf8")
        .replaceAll("\r\n", "\n")
        .split("\n")
        .forEach((line, index) => {
          const marker = line.match(/sdd-debt:\s*(.*)$/u);
          if (marker === null) return;
          const parsed = parseDebt(
            (marker[1] ?? "").replace(/\s*\*\/\s*$/u, ""),
          );
          if (parsed === null) {
            issues.push(
              createReviewIssue({
                category: "DELIBERATE_DEBT",
                severity: "MINOR",
                file,
                message: `sdd-debt 缺少 trigger 或 upgrade（第 ${index + 1} 行）`,
              }),
            );
            return;
          }
          debts.push({ file, line: index + 1, ...parsed });
        });
    } catch {
      // 删除、权限或瞬态文件不应让 advisory 扫描掩盖既有安全门禁。
    }
  }
  return { debts, issues };
}

function shouldSkip(file: string): boolean {
  return file.split("/").some((part) => EXCLUDED_DIRECTORIES.has(part));
}

function parseDebt(
  marker: string,
): Pick<DeliberateDebt, "ceiling" | "trigger" | "upgrade"> | null {
  const segments = marker.split(";").map((item) => item.trim());
  const ceiling = segments.shift()?.trim() ?? "";
  const fields = Object.fromEntries(
    segments
      .map((segment) => segment.match(/^(trigger|upgrade)=(.+)$/u))
      .filter((match): match is RegExpMatchArray => match !== null)
      .map((match) => [match[1]!, match[2]!.trim()]),
  );
  if (ceiling.length === 0 || !fields.trigger || !fields.upgrade) return null;
  return { ceiling, trigger: fields.trigger, upgrade: fields.upgrade };
}
