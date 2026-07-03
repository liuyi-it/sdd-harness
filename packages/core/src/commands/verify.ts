import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import { type TaskDefinition } from "../engines/tdd/tdd-engine.js";
import { SddError } from "../errors.js";
import { type StoredTaskResult, verifyGate } from "../quality/quality-gates.js";
import { FileLock } from "../state/file-lock.js";
import { StateStore } from "../state/state-store.js";

/**
 * verify 阶段关注“需求是否被任务覆盖，任务是否有通过的执行证据”。
 * 它不检查实现细节优雅与否，只判断是否达到可验证完成状态。
 */
export async function runVerify(root: string): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd verify");
  const store = new StateStore(root);
  try {
    const state = await store.read();
    if (state.currentPhase !== "BUILD_READY") {
      throw new SddError(
        "E_INVALID_PHASE_COMMAND",
        `无法在 ${state.currentPhase} 状态下执行 verify`,
        state.suggestedCommand ?? undefined,
      );
    }
    if (state.currentChangeId === null)
      throw new SddError("E_MISSING_CHANGE", "当前没有进行中的变更");
    const changeId = state.currentChangeId;
    const change = join(root, ".sdd", "changes", changeId);
    await store.update((current) => ({
      ...current,
      currentPhase: "VERIFYING",
      inProgressPhase: "VERIFYING",
      lastCommand: "sdd verify",
    }));
    const spec = await readFile(join(change, "spec.md"), "utf8");
    const tasks = JSON.parse(
      await readFile(join(change, "tasks.json"), "utf8"),
    ) as TaskDefinition[];
    const results = JSON.parse(
      await readFile(join(change, "task-results.json"), "utf8"),
    ) as StoredTaskResult[];
    const gate = verifyGate(spec, tasks, results, state.tasks);
    const report = reportDocument(
      "验证报告",
      [
        [
          "任务完成情况",
          gate.failures.filter((failure) => failure.includes("TASK-")),
        ],
        [
          "需求覆盖",
          gate.failures.filter((failure) => failure.includes("REQ-")),
        ],
        [
          "验收标准覆盖",
          gate.failures.filter((failure) => failure.includes("Acceptance")),
        ],
        [
          "测试结果",
          results.flatMap((result) =>
            result.verification.map(
              (entry) => `${entry.command}: ${entry.passed ? "PASS" : "FAIL"}`,
            ),
          ),
        ],
        ["边界检查", []],
        ["漂移检查", []],
        ["未通过项", gate.failures],
      ],
      gate.passed,
    );
    await new ArtifactWriter().write(join(change, "verify-report.md"), report, {
      spec,
      tasks,
      results,
    });
    if (!gate.passed)
      throw new SddError(
        "E_VERIFY_FAILED",
        gate.failures.join("; "),
        "sdd verify",
      );
    const ready = await store.update((current) => ({
      ...current,
      currentPhase: "VERIFY_READY",
      inProgressPhase: null,
      artifacts: { ...current.artifacts, verifyReport: "READY" },
      suggestedCommand: "sdd review",
    }));
    await new AuditLogger(root).write({
      command: "sdd verify",
      phase: ready.currentPhase,
      result: "PASS",
      changeId,
    });
    return {
      ok: true,
      state: ready.currentPhase,
      exitCode: 0,
      changeId,
      next: "sdd review",
    };
  } catch (error) {
    if (error instanceof SddError && error.code === "E_VERIFY_FAILED") {
      await store.update((current) => ({
        ...current,
        currentPhase: "FAILED",
        inProgressPhase: null,
        failedCommand: "sdd verify",
        lastError: error.code,
        suggestedCommand: "sdd verify",
      }));
    }
    throw error;
  } finally {
    await lock.release();
  }
}

function reportDocument(
  title: string,
  sections: Array<[string, string[]]>,
  passed: boolean,
): string {
  return [
    `# ${title}`,
    "",
    "## 概要",
    "",
    passed ? "所有质量闸门均已通过。" : "存在一个或多个未通过的质量闸门。",
    "",
    ...sections.flatMap(([heading, items]) => [
      `## ${heading}`,
      "",
      ...(items.length === 0 ? ["PASS"] : items.map((item) => `- ${item}`)),
      "",
    ]),
    "## Result",
    "",
    passed ? "PASS" : "FAIL",
  ].join("\n");
}
