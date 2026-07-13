import { describe, expect, it } from "vitest";

import { validateTaskResult } from "../src/index.js";

const validResult = () => ({
  schemaVersion: "1.2.0",
  taskId: "TASK-001",
  status: "SUCCEEDED",
  modifiedFiles: ["src/example.ts"],
  createdFiles: [],
  commandsRun: [
    {
      command: "npm",
      args: ["test"],
      exitCode: 0,
      passed: true,
      outputSummary: "通过",
    },
  ],
  tddEvidence: [],
  verification: [],
  notes: ["完成"],
});

describe("validateTaskResult", () => {
  it.each([
    [
      "commandsRun args",
      (value: ReturnType<typeof validResult>) => {
        value.commandsRun[0]!.args = [1 as unknown as string];
      },
    ],
    [
      "notes",
      (value: ReturnType<typeof validResult>) => {
        value.notes = [false as unknown as string];
      },
    ],
  ])("拒绝包含非字符串元素的 %s", (_name, mutate) => {
    const value = validResult();
    mutate(value);
    expect(() => validateTaskResult(value)).toThrow(
      "E_SCHEMA_VALIDATION_FAILED",
    );
  });

  it("接受深度类型合法的结果", () => {
    expect(validateTaskResult(validResult())).toMatchObject({
      taskId: "TASK-001",
    });
  });
});
