import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { runDeterministicReview } from "../src/quality/deterministic-review.js";
import {
  createReviewIssue,
  createReviewReport,
  isBlocking,
  renderReviewMarkdown,
  REVIEW_CATEGORIES,
  stableId,
  writeReviewReport,
} from "../src/quality/review-report.js";
import type { TaskDefinition } from "../src/engines/tdd/tdd-engine.js";
import type { GitSnapshot } from "../src/git/git-inspector.js";
import type { SpecDocument } from "../src/engines/openspec/model.js";

const spec: SpecDocument = {
  schemaVersion: "1.2.0",
  title: "Order service",
  source: "spec.md",
  requirements: [],
};

function task(partial: Partial<TaskDefinition>): TaskDefinition {
  return {
    id: "TASK-001",
    title: "demo",
    phase: "RED",
    status: "PENDING",
    requirements: ["REQ-001"],
    scenarios: ["SC-001"],
    dependsOn: [],
    allowedFiles: ["src/**"],
    expectedNewFiles: [],
    forbiddenFiles: [],
    verification: ["npm test"],
    doneCriteria: ["done"],
    ...partial,
  };
}

function snapshot(files: Record<string, string>): GitSnapshot {
  const entries = Object.entries(files);
  const sorted = entries.map(([f]) => f).sort();
  return {
    available: true,
    files: sorted,
    hashes: Object.fromEntries(entries),
  };
}

describe("ReviewReport v2 + deterministic review", () => {
  const roots: string[] = [];
  afterEach(async () => {
    const { rm } = await import("node:fs/promises");
    await Promise.all(roots.splice(0).map((r) => rm(r, { recursive: true })));
  });

  it("emits FILE_SCOPE issue when current diff touches a forbidden file", () => {
    const out = runDeterministicReview({
      tasks: [task({ forbiddenFiles: ["secrets.env"] })],
      results: [
        {
          taskId: "TASK-001",
          modifiedFiles: ["secrets.env"],
          tddEvidence: [],
          verification: [{ command: "npm test", passed: true, output: "ok" }],
        },
      ],
      baseline: snapshot({}),
      current: snapshot({ "secrets.env": "x" }),
      spec,
    });
    expect(
      out.issues.some(
        (i) => i.category === "FILE_SCOPE" && i.file === "secrets.env",
      ),
    ).toBe(true);
  });

  it("does not emit FILE_SCOPE for forbidden files absent from current diff", () => {
    const out = runDeterministicReview({
      tasks: [task({ forbiddenFiles: [".env"] })],
      results: [
        {
          taskId: "TASK-001",
          modifiedFiles: ["src/index.ts"],
          tddEvidence: [],
          verification: [{ command: "npm test", passed: true, output: "ok" }],
        },
      ],
      baseline: snapshot({ "src/index.ts": "old" }),
      current: snapshot({ "src/index.ts": "x" }),
      spec,
    });
    expect(out.issues.some((i) => i.category === "FILE_SCOPE")).toBe(false);
  });

  it("emits UNRELATED_CHANGE issue when current diff contains file not in modifiedFiles", () => {
    const baseline = snapshot({ "src/index.ts": "old" });
    const current = snapshot({ "src/index.ts": "new", "src/unknown.ts": "x" });
    const out = runDeterministicReview({
      tasks: [task()],
      results: [
        {
          taskId: "TASK-001",
          modifiedFiles: ["src/index.ts"],
          tddEvidence: [],
          verification: [{ command: "npm test", passed: true, output: "ok" }],
        },
      ],
      baseline,
      current,
      spec,
    });
    expect(out.issues.map((i) => i.file)).toContain("src/unknown.ts");
    expect(out.issues.find((i) => i.file === "src/unknown.ts")?.category).toBe(
      "UNRELATED_CHANGE",
    );
  });

  it("emits TESTING issue for failed verification evidence", () => {
    const out = runDeterministicReview({
      tasks: [task()],
      results: [
        {
          taskId: "TASK-001",
          modifiedFiles: [],
          tddEvidence: [],
          verification: [
            { command: "npm test", passed: false, output: "fail" },
          ],
        },
      ],
      baseline: snapshot({}),
      current: snapshot({}),
      spec,
    });
    expect(out.issues.some((i) => i.category === "TESTING")).toBe(true);
  });

  it("emits BLOCKER when verification evidence contains 'rm'", () => {
    const out = runDeterministicReview({
      tasks: [task()],
      results: [
        {
          taskId: "TASK-001",
          modifiedFiles: [],
          tddEvidence: [],
          verification: [{ command: "rm -rf /", passed: true, output: "ran" }],
        },
      ],
      baseline: snapshot({}),
      current: snapshot({}),
      spec,
    });
    expect(out.issues.some((i) => i.category === "BLOCKER")).toBe(true);
  });

  it("ReviewIssue IDs are deterministic and stable", () => {
    const a = createReviewIssue({
      category: "SECRET_LEAK",
      severity: "MAJOR",
      file: "src/x.ts",
      message: "leaks aws key",
    });
    const b = createReviewIssue({
      category: "SECRET_LEAK",
      severity: "MAJOR",
      file: "src/x.ts",
      message: "leaks aws key",
    });
    expect(a.id).toBe(b.id);
    expect(
      stableId({
        category: "SECRET_LEAK",
        severity: "MAJOR",
        message: "leaks aws key",
        file: "src/x.ts",
      }),
    ).toBe(a.id);
  });

  it("aggregates severity + category counts", () => {
    const report = createReviewReport({
      changeId: "demo",
      issues: [
        createReviewIssue({
          category: "BLOCKER",
          severity: "MAJOR",
          message: "x1",
        }),
        createReviewIssue({
          category: "FILE_SCOPE",
          severity: "MAJOR",
          message: "y",
        }),
        createReviewIssue({
          category: "TESTING",
          severity: "MINOR",
          message: "z",
        }),
        createReviewIssue({
          category: "BLOCKER",
          severity: "MAJOR",
          message: "x2",
        }),
      ],
    });
    expect(report.severityCounts.MAJOR).toBe(3);
    expect(report.severityCounts.MINOR).toBe(1);
    expect(report.severityCounts.INFO).toBe(0);
    expect(report.categoryCounts.BLOCKER).toBe(2);
    expect(report.categoryCounts.FILE_SCOPE).toBe(1);
    expect(report.result).toBe("BLOCK");
    expect(report.standards.status).toBe("FAILED");
    expect(report.spec.status).toBe("PASSED");
    expect(report.summary).toEqual({
      standardsFindingCount: 4,
      specFindingCount: 0,
    });
  });

  it("isBlocking returns true when MAJOR appears in BLOCKER category", () => {
    const report = createReviewReport({
      changeId: "demo",
      issues: [
        createReviewIssue({
          category: "BLOCKER",
          severity: "MAJOR",
          message: "x1",
        }),
        createReviewIssue({
          category: "TESTING",
          severity: "MINOR",
          message: "y",
        }),
      ],
    });
    expect(isBlocking(report.issues)).toBe(true);
    expect(report.result).toBe("BLOCK");
  });

  it("isBlocking returns false for MINOR-only issues", () => {
    const report = createReviewReport({
      changeId: "demo",
      issues: [
        createReviewIssue({
          category: "TESTING",
          severity: "MINOR",
          message: "y",
        }),
      ],
    });
    expect(isBlocking(report.issues)).toBe(false);
    expect(report.result).toBe("PASS");
  });

  it("renderMarkdown includes summary, severity counts, and issues", () => {
    const report = createReviewReport({
      changeId: "demo",
      issues: [
        createReviewIssue({
          category: "FILE_SCOPE",
          severity: "MAJOR",
          message: "blocked file",
        }),
      ],
    });
    const md = renderReviewMarkdown(report);
    expect(md).toContain("# 审查报告 (v2)");
    expect(md).toContain("MAJOR=1");
    expect(md).toContain("FILE_SCOPE");
  });

  (process.platform === "win32" ? it.skip : it)(
    "writeReviewReport persists JSON + Markdown atomically",
    async () => {
      const root = await mkdtemp(join(tmpdir(), "sdd-rreport-"));
      roots.push(root);
      const report = createReviewReport({
        changeId: "demo",
        issues: [
          createReviewIssue({
            category: "BLOCKER",
            severity: "MAJOR",
            message: "x",
          }),
        ],
      });
      const paths = await writeReviewReport(root, "demo", report);
      expect(
        paths.jsonPath.endsWith(".sdd/changes/demo/review-report.v2.json"),
      ).toBe(true);
      expect(
        paths.mdPath.endsWith(".sdd/changes/demo/review-report.v2.md"),
      ).toBe(true);
      const stored = JSON.parse(await readFile(paths.jsonPath, "utf8"));
      expect(stored.result).toBe("BLOCK");
    },
  );

  it("uses the documented category whitelist", () => {
    expect(REVIEW_CATEGORIES).toEqual([
      "FILE_SCOPE",
      "UNRELATED_CHANGE",
      "SECURITY",
      "TESTING",
      "BLOCKER",
      "SECRET_LEAK",
    ]);
  });

  it("falls back to PASS when issues are empty", () => {
    const report = createReviewReport({ changeId: "demo", issues: [] });
    expect(report.result).toBe("PASS");
    expect(report.severityCounts).toEqual({ MAJOR: 0, MINOR: 0, INFO: 0 });
  });

  it("将规格覆盖缺口归入 Spec 轴并独立阻断", () => {
    const out = runDeterministicReview({
      tasks: [],
      results: [],
      baseline: snapshot({}),
      current: snapshot({}),
      spec: {
        title: "订单",
        requirements: [
          {
            id: "REQ-001",
            title: "取消订单",
            statement: "系统 SHALL 允许取消订单",
            operation: "ADDED",
            scenarios: [
              {
                id: "REQ-001-SC-001",
                title: "成功取消",
                given: ["订单待处理"],
                when: ["用户取消"],
                then: ["订单已取消"],
              },
            ],
          },
        ],
      },
    });
    const report = createReviewReport({
      changeId: "demo",
      fixedPoint: "sha256:test",
      issues: out.issues,
    });

    expect(report.standards.status).toBe("PASSED");
    expect(report.spec.status).toBe("FAILED");
    expect(report.spec.findings).toHaveLength(2);
    expect(report.spec.findings.every((issue) => issue.axis === "SPEC")).toBe(
      true,
    );
    expect(report.result).toBe("BLOCK");
  });
});
