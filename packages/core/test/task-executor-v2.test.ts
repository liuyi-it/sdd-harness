import { describe, expect, it } from "vitest";

import {
  normalizeTaskExecutionResult,
  type NormalizedTaskExecutionArtifact,
} from "../src/build/task-result-normalizer.js";

describe("task result normalizer", () => {
  it("补齐 v1 结果为 1.2.0 运行级制品", () => {
    const artifact = normalizeTaskExecutionResult(
      {
        taskId: "TASK-001-RED",
        modifiedFiles: ["src/order.ts"],
        tddEvidence: [
          {
            phase: "RED",
            command: "npm test -- packages/core/test/auto.test.ts",
            passed: false,
            expectedFailure: true,
            output: "预期失败",
          },
        ],
        verification: [],
      },
      {
        actualFileDelta: {
          added: [],
          modified: ["src/order.ts"],
          deleted: [],
        },
        startedAt: "2026-07-06T00:00:00.000Z",
        endedAt: "2026-07-06T00:00:01.000Z",
        requestedMode: "subagent",
        actualMode: "main-agent",
        degradedReason: "宿主不支持 subagent，已降级为主代理执行",
      },
    );

    expect(artifact).toMatchObject({
      schemaVersion: "1.2.0",
      taskId: "TASK-001-RED",
      status: "SUCCEEDED",
      mode: {
        requested: "subagent",
        actual: "main-agent",
      },
      fileDelta: {
        added: [],
        modified: ["src/order.ts"],
        deleted: [],
      },
      notes: expect.arrayContaining([
        "宿主不支持 subagent，已降级为主代理执行",
      ]),
      commandEvidence: [
        {
          command: "npm",
          args: ["test", "--", "packages/core/test/auto.test.ts"],
          outputSummary: "预期失败",
        },
      ],
    });
  });

  it("保留 v2 结构化结果，并使用 Git delta 裁决 fileDelta", () => {
    const artifact = normalizeTaskExecutionResult(
      {
        schemaVersion: "1.2.0",
        taskId: "TASK-001-VERIFY",
        status: "SUCCEEDED",
        summary: "VERIFY 已通过",
        commandEvidence: [
          {
            command: "npm",
            args: ["test"],
            outputSummary: "1 passed",
          },
        ],
        fileDelta: {
          added: ["declared.ts"],
          modified: [],
          deleted: [],
        },
        timestamps: {
          startedAt: "2026-07-06T00:00:00.000Z",
          endedAt: "2026-07-06T00:00:01.000Z",
        },
      },
      {
        actualFileDelta: {
          added: [],
          modified: ["src/order.ts"],
          deleted: [],
        },
        startedAt: "2026-07-06T00:00:00.000Z",
        endedAt: "2026-07-06T00:00:01.000Z",
        requestedMode: "main-agent",
        actualMode: "main-agent",
      },
    );

    expect(artifact.fileDelta).toEqual({
      added: [],
      modified: ["src/order.ts"],
      deleted: [],
    });
    expect(artifact.commandEvidence[0]).toEqual({
      command: "npm",
      args: ["test"],
      outputSummary: "1 passed",
    });
  });

  it("危险字符串命令会被安全阻断", () => {
    expect(() =>
      normalizeTaskExecutionResult(
        {
          taskId: "TASK-001-RED",
          modifiedFiles: [],
          tddEvidence: [
            {
              phase: "RED",
              command: "npm test | cat",
              passed: false,
              expectedFailure: true,
              output: "预期失败",
            },
          ],
          verification: [],
        },
        {
          actualFileDelta: { added: [], modified: [], deleted: [] },
          startedAt: "2026-07-06T00:00:00.000Z",
          endedAt: "2026-07-06T00:00:01.000Z",
          requestedMode: "main-agent",
          actualMode: "main-agent",
        },
      ),
    ).toThrowError(expect.objectContaining({ code: "E_SECURITY_BLOCKED" }));
  });

  it("导出的制品类型保持稳定", () => {
    const artifact: NormalizedTaskExecutionArtifact =
      normalizeTaskExecutionResult(
        {
          taskId: "TASK-001-GREEN",
          modifiedFiles: ["src/order.ts"],
          tddEvidence: [
            {
              phase: "GREEN",
              command: "npm test",
              passed: true,
              output: "通过",
            },
          ],
          verification: [],
        },
        {
          actualFileDelta: {
            added: [],
            modified: ["src/order.ts"],
            deleted: [],
          },
          startedAt: "2026-07-06T00:00:00.000Z",
          endedAt: "2026-07-06T00:00:01.000Z",
          requestedMode: "main-agent",
          actualMode: "main-agent",
        },
      );

    expect(artifact.summary).toContain("GREEN");
  });
});
