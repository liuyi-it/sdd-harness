import { describe, expect, it } from "vitest";

import type { TaskExecutionResult } from "../src/build/task-executor.js";
import type {
  TaskDefinition,
  TddPhase,
} from "../src/engines/tdd/tdd-engine.js";
import { tddChainFailures } from "../src/quality/tdd-evidence.js";

const phases: TddPhase[] = ["RED", "GREEN", "REFACTOR", "VERIFY"];

function chain(): {
  tasks: TaskDefinition[];
  results: Array<TaskExecutionResult & { taskId: string }>;
} {
  const tasks = phases.map((phase, index) => ({
    id: `TASK-${phase}`,
    title: phase,
    phase,
    status: "PENDING" as const,
    requirements: ["REQ-001"],
    scenarios: ["SCN-001"],
    dependsOn: index === 0 ? [] : [`TASK-${phases[index - 1]}`],
    allowedFiles: ["src/**"],
    expectedNewFiles: [],
    forbiddenFiles: [],
    verification: ["npm test"],
    doneCriteria: ["done"],
  }));
  const results = tasks.map((task) => ({
    taskId: task.id,
    modifiedFiles: [],
    tddEvidence: [
      task.phase === "RED"
        ? {
            phase: task.phase,
            command: "npm test",
            passed: false,
            expectedFailure: true,
            output: "failed",
          }
        : {
            phase: task.phase,
            command: "npm test",
            passed: true,
            output: "passed",
          },
    ],
    verification:
      task.phase === "VERIFY"
        ? [{ command: "npm test", passed: true, output: "passed" }]
        : [],
  }));
  return { tasks, results };
}

describe("TDD 阶段链", () => {
  it.each([
    ["缺阶段", (value: ReturnType<typeof chain>) => value.tasks.pop()],
    ["顺序错", (value: ReturnType<typeof chain>) => value.tasks.reverse()],
    [
      "重复 phase",
      (value: ReturnType<typeof chain>) => {
        value.tasks[1]!.phase = "RED";
      },
    ],
    [
      "requirement 集合不一致",
      (value: ReturnType<typeof chain>) => {
        value.tasks[1]!.requirements = ["REQ-002"];
      },
    ],
    [
      "scenario 集合不一致",
      (value: ReturnType<typeof chain>) => {
        value.tasks[1]!.scenarios = ["SCN-002"];
      },
    ],
  ])("拒绝%s", (_name, mutate) => {
    const value = chain();
    mutate(value);
    expect(tddChainFailures(value.tasks, value.results)).not.toEqual([]);
  });

  it.each([1, 2, 3])("拒绝阶段 %i 缺少直接前驱依赖", (index) => {
    const value = chain();
    value.tasks[index]!.dependsOn = [];
    expect(tddChainFailures(value.tasks, value.results)).not.toEqual([]);
  });
});
