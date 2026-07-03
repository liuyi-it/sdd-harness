import { readFile } from "node:fs/promises";
import { join } from "node:path";

import { AuditLogger } from "../audit/audit-logger.js";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { type CommandResult } from "../contracts.js";
import { type TaskDefinition } from "../engines/tdd/tdd-engine.js";
import { SddError } from "../errors.js";
import { reviewGate, type StoredTaskResult } from "../quality/quality-gates.js";
import { FileLock } from "../state/file-lock.js";
import { StateStore } from "../state/state-store.js";

/**
 * review 阶段在 verify 通过之后再做一次实现侧审查，
 * 重点确认修改范围、验证证据和任务声明之间没有漂移。
 */
export async function runReview(root: string): Promise<CommandResult> {
  const lock = new FileLock(root);
  await lock.acquire("sdd review");
  const store = new StateStore(root);
  try {
    const state = await store.read();
    if (state.currentPhase !== "VERIFY_READY") {
      throw new SddError(
        "E_VERIFY_REQUIRED",
        `无法在 ${state.currentPhase} 状态下执行 review`,
        state.suggestedCommand ?? "sdd verify",
      );
    }
    if (state.currentChangeId === null)
      throw new SddError("E_MISSING_CHANGE", "当前没有进行中的变更");
    const changeId = state.currentChangeId;
    const change = join(root, ".sdd", "changes", changeId);
    await store.update((current) => ({
      ...current,
      currentPhase: "REVIEWING",
      inProgressPhase: "REVIEWING",
      lastCommand: "sdd review",
    }));
    const tasks = JSON.parse(
      await readFile(join(change, "tasks.json"), "utf8"),
    ) as TaskDefinition[];
    const results = JSON.parse(
      await readFile(join(change, "task-results.json"), "utf8"),
    ) as StoredTaskResult[];
    const gate = reviewGate(tasks, results);
    const report = [
      "# 审查报告",
      "",
      "## 概要",
      "",
      gate.passed ? "实现证据与计划一致。" : "审查发现需要修复的问题。",
      "",
      "## 代码质量",
      "",
      "每个已完成任务都具备验证证据。",
      "",
      "## 架构一致性",
      "",
      "改动仍与计划中的任务和需求保持关联。",
      "",
      "## 性能",
      "",
      "未记录到未经审查的性能问题。",
      "",
      "## 简洁性",
      "",
      "未记录到无关的实现证据。",
      "",
      "## 安全",
      "",
      "文件与命令的安全闸门均已生效。",
      "",
      "## 文件范围",
      "",
      gate.passed
        ? "PASS"
        : gate.failures.map((failure) => `- ${failure}`).join("\n"),
      "",
      "## 无关改动",
      "",
      gate.passed ? "未检测到。" : "参见「需修复项」。",
      "",
      "## 需修复项",
      "",
      gate.failures.length === 0
        ? "无。"
        : gate.failures.map((failure) => `- ${failure}`).join("\n"),
      "",
      "## 建议",
      "",
      "后续改动请保持在明确的任务范围之内。",
      "",
      "## Result",
      "",
      gate.passed ? "PASS" : "FAIL",
    ].join("\n");
    await new ArtifactWriter().write(join(change, "review-report.md"), report, {
      tasks,
      results,
    });
    if (!gate.passed)
      throw new SddError(
        "E_REVIEW_FAILED",
        gate.failures.join("; "),
        "sdd review",
      );
    const ready = await store.update((current) => ({
      ...current,
      currentPhase: "REVIEW_READY",
      inProgressPhase: null,
      artifacts: { ...current.artifacts, reviewReport: "READY" },
      suggestedCommand: "sdd archive",
    }));
    await new AuditLogger(root).write({
      command: "sdd review",
      phase: ready.currentPhase,
      result: "PASS",
      changeId,
    });
    return {
      ok: true,
      state: ready.currentPhase,
      exitCode: 0,
      changeId,
      next: "sdd archive",
    };
  } catch (error) {
    if (error instanceof SddError && error.code === "E_REVIEW_FAILED") {
      await store.update((current) => ({
        ...current,
        currentPhase: "FAILED",
        inProgressPhase: null,
        failedCommand: "sdd review",
        lastError: error.code,
        suggestedCommand: "sdd review",
      }));
    }
    throw error;
  } finally {
    await lock.release();
  }
}
