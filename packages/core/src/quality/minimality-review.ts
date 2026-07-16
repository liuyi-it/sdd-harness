import type { PlannedDependency } from "../engines/superpowers/protocol.js";
import type { GitSnapshot } from "../git/git-inspector.js";
import type { StoredTaskResult } from "./quality-gates.js";
import {
  collectChangeComplexityMetrics,
  type ChangeComplexityMetrics,
} from "./change-complexity.js";
import {
  collectDependencyDelta,
  type DependencyDelta,
} from "./dependency-delta.js";
import { scanDeliberateDebt, type DeliberateDebt } from "./deliberate-debt.js";
import { createReviewIssue, type ReviewIssue } from "./review-report.js";

export interface MinimalityReviewInput {
  root: string;
  baseline: GitSnapshot;
  current: GitSnapshot;
  plannedDependencies: PlannedDependency[];
  taskResults: readonly StoredTaskResult[];
}

export interface MinimalityReviewResult {
  issues: ReviewIssue[];
  metrics: ChangeComplexityMetrics;
  dependencies: DependencyDelta[];
  debts: DeliberateDebt[];
}

export async function runMinimalityReview(
  input: MinimalityReviewInput,
): Promise<MinimalityReviewResult> {
  const dependency = collectDependencyDelta(input.baseline, input.current);
  const debt = await scanDeliberateDebt(
    input.root,
    input.baseline,
    input.current,
  );
  const rawMetrics = await collectChangeComplexityMetrics(
    input.root,
    input.baseline,
    input.current,
  );
  const metrics: ChangeComplexityMetrics = {
    ...rawMetrics,
    dependenciesAdded: dependency.dependencies.filter(
      ({ change }) => change === "ADDED",
    ).length,
    dependenciesRemoved: dependency.dependencies.filter(
      ({ change }) => change === "REMOVED",
    ).length,
    deliberateDebtCount: debt.debts.length,
  };
  return {
    issues: dependency.issues.concat(
      dependency.dependencies.flatMap((item) =>
        dependencyIssues(item, input.plannedDependencies),
      ),
      debt.issues,
      advisoryComplexityIssues(input.taskResults),
    ),
    metrics,
    dependencies: dependency.dependencies,
    debts: debt.debts,
  };
}

function advisoryComplexityIssues(
  results: readonly StoredTaskResult[],
): ReviewIssue[] {
  return results.flatMap((result) =>
    (result.minimality?.abstractionsAdded ?? []).flatMap((abstraction) =>
      abstraction.consumers.length >= 2
        ? []
        : [
            createReviewIssue({
              category: "COMPLEXITY",
              severity: "MINOR",
              deterministic: false,
              file: abstraction.file,
              task: result.taskId,
              message: `抽象 ${abstraction.name} 只有 ${abstraction.consumers.length} 个消费者；请确认能否直接实现以减少一层转发`,
            }),
          ],
    ),
  );
}

function dependencyIssues(
  delta: DependencyDelta,
  planned: readonly PlannedDependency[],
): ReviewIssue[] {
  if (delta.change === "ADDED") {
    const declared = planned.some(
      (item) =>
        item.action === "ADD" &&
        item.name === delta.name &&
        normalizeManifestPath(item.manifest) ===
          normalizeManifestPath(delta.manifest),
    );
    return declared
      ? []
      : [
          createReviewIssue({
            category: "UNPLANNED_DEPENDENCY",
            severity: "MAJOR",
            file: delta.manifest,
            message: `新增依赖 ${delta.name} 未在 plan.json.dependencies 中声明`,
          }),
        ];
  }
  if (delta.change === "UPDATED")
    return [
      createReviewIssue({
        category: "UNPLANNED_DEPENDENCY",
        severity: isMajorUpgrade(delta.before, delta.after) ? "MAJOR" : "MINOR",
        file: delta.manifest,
        message: `依赖 ${delta.name} 从 ${delta.before} 更新为 ${delta.after}，请确认迁移影响`,
      }),
    ];
  return [
    createReviewIssue({
      category: "UNPLANNED_DEPENDENCY",
      severity: "INFO",
      file: delta.manifest,
      message: `已删除依赖 ${delta.name}`,
    }),
  ];
}

function normalizeManifestPath(path: string): string {
  return path.replaceAll("\\", "/");
}

function isMajorUpgrade(before: string | null, after: string | null): boolean {
  const beforeMajor = majorVersion(before);
  const afterMajor = majorVersion(after);
  return (
    beforeMajor !== null && afterMajor !== null && afterMajor > beforeMajor
  );
}

function majorVersion(version: string | null): number | null {
  const match = /^\D*(\d+)/u.exec(version ?? "");
  return match === null ? null : Number(match[1]);
}
