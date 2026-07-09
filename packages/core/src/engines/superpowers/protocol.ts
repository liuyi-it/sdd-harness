export type TddPhase = "RED" | "GREEN" | "REFACTOR" | "VERIFY";

export interface TaskDefinition {
  id: string;
  title: string;
  phase: TddPhase;
  status: "PENDING" | "BUILDING" | "DONE" | "FAILED" | "SKIPPED";
  requirements: string[];
  scenarios: string[];
  dependsOn: string[];
  allowedFiles: string[];
  expectedNewFiles: string[];
  forbiddenFiles: string[];
  verification: string[];
  doneCriteria: string[];
}

export interface PlanArtifacts {
  tasks: TaskDefinition[];
  tasksMarkdown: string;
  testPlan: string;
  context: string;
  contextPacks: Record<string, string>;
}

export interface PlanningInput {
  spec: string;
  design: string;
  impact: string;
  codebaseSummary: string;
  existingPlan?: {
    tasksMarkdown: string;
    testPlan: string;
    context: string;
  };
}
