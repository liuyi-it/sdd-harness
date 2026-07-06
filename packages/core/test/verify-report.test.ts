import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  classifyFailure,
  createVerifyReport,
  renderMarkdown,
  VERIFY_FAILURE_CODES,
  VERIFY_REPORT_LEVELS,
  writeVerifyReport,
} from "../src/quality/verify-report.js";

describe("VerifyReport v1.2", () => {
  const roots: string[] = [];

  beforeEach(() => {});
  afterEach(async () => {
    const { rm } = await import("node:fs/promises");
    await Promise.all(roots.splice(0).map((r) => rm(r, { recursive: true })));
  });

  it("uses exactly the seven expected levels", () => {
    expect(VERIFY_REPORT_LEVELS).toEqual([
      "artifacts",
      "tasks",
      "requirements",
      "scenarios",
      "tddEvidence",
      "tests",
      "drift",
    ]);
  });

  it("creates PASS report when no failures", () => {
    const report = createVerifyReport({
      changeId: "demo",
      counts: { requirements: 1, scenarios: 2, tasks: 4, tests: 4 },
      failures: [],
    });
    expect(report.result).toBe("PASS");
    expect(report.result).toBe("PASS");
    expect(report.levels.drift.passed).toBe(true);
    expect(report.levels.drift.failures).toHaveLength(0);
    expect(report.schemaVersion).toBe("1.2.0");
  });

  it("creates FAIL report and routes failures to correct levels", () => {
    const failures = [
      classifyFailure("tasks", "TASK-001 未完成", "TASK-001"),
      classifyFailure("requirements", "REQ-001 未关联到任何任务", "REQ-001"),
      classifyFailure(
        "scenarios",
        "REQ-001/SC-001 缺少 RED 阶段",
        "REQ-001/SC-001",
      ),
      classifyFailure(
        "tddEvidence",
        "TASK-001 未证明观察到预期失败",
        "TASK-001",
      ),
      classifyFailure("drift", "未跟踪到任务结果的变更文件：package.json"),
      classifyFailure("tests", "验证命令未在允许清单内", "TASK-001"),
    ];
    const report = createVerifyReport({
      changeId: "demo",
      counts: { requirements: 1, scenarios: 2, tasks: 4, tests: 4 },
      failures,
    });
    expect(report.result).toBe("FAIL");
    expect(report.levels.drift.passed).toBe(false);
    expect(report.levels.drift.failures[0]?.code).toBe(
      VERIFY_FAILURE_CODES.DRIFT_DETECTED,
    );
    expect(report.levels.tddEvidence.failures[0]?.entity).toBe("TASK-001");
    expect(report.summary).toContain("drift=1");
  });

  it("classifyFailure picks stable failure code per level", () => {
    expect(classifyFailure("artifacts", "spec.md 缺失").code).toBe(
      VERIFY_FAILURE_CODES.ARTIFACT_MISSING,
    );
    expect(classifyFailure("tasks", "TASK-001 缺少执行证据").code).toBe(
      VERIFY_FAILURE_CODES.TASK_INCOMPLETE,
    );
    expect(classifyFailure("tests", "验证失败").code).toBe(
      VERIFY_FAILURE_CODES.TEST_FAILED,
    );
  });

  it("renderMarkdown lists every level with status", () => {
    const report = createVerifyReport({
      changeId: "demo",
      counts: { requirements: 1, scenarios: 2, tasks: 4, tests: 4 },
      failures: [classifyFailure("tasks", "TASK-001 缺少执行证据", "TASK-001")],
    });
    const md = renderMarkdown(report);
    for (const level of VERIFY_REPORT_LEVELS)
      expect(md).toContain(`## ${level}`);
    expect(md).toContain("E_VERIFY_TASK_INCOMPLETE");
  });

  it("writeVerifyReport persists JSON + Markdown atomically", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-vreport-"));
    roots.push(root);
    const changeDir = join(root, ".sdd", "changes", "demo");
    const report = createVerifyReport({
      changeId: "demo",
      counts: { requirements: 1, scenarios: 1, tasks: 1, tests: 1 },
      failures: [],
    });
    const paths = await writeVerifyReport(root, "demo", report);
    expect(paths.jsonPath).toBe(join(changeDir, "verify-report.v1.2.json"));
    expect(paths.mdPath).toBe(join(changeDir, "verify-report.v1.2.md"));
    const stored = JSON.parse(await readFile(paths.jsonPath, "utf8"));
    expect(stored).toMatchObject({ changeId: "demo", result: "PASS" });
    const md = await readFile(paths.mdPath, "utf8");
    expect(md).toContain("验证报告 (v1.2)");
  });

  it("writeVerifyReport requires version-stable failure codes only", () => {
    const failures = [
      classifyFailure("requirements", "REQ-001 uncovered", "REQ-001"),
      classifyFailure("scenarios", "SC-001 missing", "SC-001"),
      classifyFailure("tddEvidence", "RED not observed", "TASK-001"),
      classifyFailure("drift", "extra file", "package.json"),
      classifyFailure("artifacts", "design.md missing"),
      classifyFailure("tests", "tests failed", "TASK-001"),
      classifyFailure("tasks", "TASK-001 not DONE", "TASK-001"),
    ];
    const codes = failures.map((f) => f.code);
    expect(new Set(codes).size).toBe(failures.length);
    expect(codes).toEqual([
      VERIFY_FAILURE_CODES.REQUIREMENT_UNCOVERED,
      VERIFY_FAILURE_CODES.SCENARIO_MISSING,
      VERIFY_FAILURE_CODES.EVIDENCE_INCOMPLETE,
      VERIFY_FAILURE_CODES.DRIFT_DETECTED,
      VERIFY_FAILURE_CODES.ARTIFACT_MISSING,
      VERIFY_FAILURE_CODES.TEST_FAILED,
      VERIFY_FAILURE_CODES.TASK_INCOMPLETE,
    ]);
  });
});
