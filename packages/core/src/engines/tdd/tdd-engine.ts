export interface TaskDefinition {
  id: string;
  title: string;
  status: "PENDING" | "BUILDING" | "DONE" | "FAILED" | "SKIPPED";
  requirements: string[];
  dependsOn: string[];
  allowedFiles: string[];
  expectedNewFiles: string[];
  forbiddenFiles: string[];
  verification: string[];
  doneCriteria: string[];
}

interface DesignInput {
  spec: string;
  impact: string;
  codebaseSummary: string;
  packageStructure: string;
  architecture: string;
}

interface PlanInput {
  spec: string;
  design: string;
  impact: string;
  codebaseSummary: string;
}

export interface PlanArtifacts {
  tasks: TaskDefinition[];
  tasksMarkdown: string;
  testPlan: string;
  context: string;
  contextPacks: Record<string, string>;
}

export class TddEngine {
  generateDesign(input: DesignInput): string {
    return [
      "# Design",
      "",
      "## Current Code Structure",
      "",
      input.codebaseSummary,
      "",
      input.packageStructure,
      "",
      "## Target Design",
      "",
      "Implement the specification through the existing project boundaries. Keep command, domain, persistence, and test responsibilities isolated.",
      "",
      "## Affected Modules and Files",
      "",
      input.architecture,
      "",
      "## API Changes",
      "",
      "Expose only the API behavior explicitly required by the specification; validate inputs and preserve compatibility.",
      "",
      "## Data Changes",
      "",
      "Persist only required state. Any schema change requires a forward migration and tested rollback.",
      "",
      "## Transaction and Idempotency",
      "",
      "Apply state changes atomically, reject invalid transitions, and make repeated requests safe.",
      "",
      "## Error Handling",
      "",
      "Return stable domain errors for missing, unauthorized, conflicting, and invalid operations.",
      "",
      "## Logging and Monitoring",
      "",
      "Record security-relevant state transitions without secrets or full source content.",
      "",
      "## Testing Strategy",
      "",
      "Use test-first unit coverage, integration coverage at module boundaries, and end-to-end acceptance scenarios.",
      "",
      "## Risks and Rollback",
      "",
      "Risks include authorization gaps, stale state, concurrent updates, and incompatible data changes. Roll back code and migrations together.",
      "",
      "## Specification Reference",
      "",
      input.spec,
      "",
      "## Impact Reference",
      "",
      input.impact,
    ].join("\n");
  }

  generatePlan(input: PlanInput): PlanArtifacts {
    const requirements = extractRequirements(input.spec);
    const task: TaskDefinition = {
      id: "TASK-001",
      title: "Implement and verify the specified behavior",
      status: "PENDING",
      requirements: requirements.length === 0 ? ["REQ-001"] : requirements,
      dependsOn: [],
      allowedFiles: ["src/**", "packages/**", "test/**", "tests/**", "docs/**"],
      expectedNewFiles: ["src/**", "packages/**", "test/**", "tests/**"],
      forbiddenFiles: [".git/**", ".env", "**/credentials*"],
      verification: ["npm test"],
      doneCriteria: [
        "All linked requirements and acceptance criteria are implemented",
        "Success, failure, security, and concurrency paths are tested",
        "No files outside the allowed scope are modified",
      ],
    };
    const tasksMarkdown = [
      "# Tasks",
      "",
      `## ${task.id}: ${task.title}`,
      "",
      `Status: ${task.status}`,
      "",
      "Requirements:",
      ...task.requirements.map((requirement) => `- ${requirement}`),
      "",
      "Depends On:",
      "- None",
      "",
      "Allowed Files:",
      ...task.allowedFiles.map((file) => `- ${file}`),
      "",
      "Expected New Files:",
      ...task.expectedNewFiles.map((file) => `- ${file}`),
      "",
      "Forbidden Files:",
      ...task.forbiddenFiles.map((file) => `- ${file}`),
      "",
      "Verification:",
      ...task.verification.map((command) => `- ${command}`),
      "",
      "Done Criteria:",
      ...task.doneCriteria.map((criterion) => `- ${criterion}`),
    ].join("\n");
    const context = [
      "# Change Context",
      "",
      "## Codebase",
      "",
      input.codebaseSummary,
      "",
      "## Impact",
      "",
      input.impact,
      "",
      "## Design",
      "",
      input.design,
    ].join("\n");
    const contextPack = [
      `# Context Pack: ${task.id}`,
      "",
      "## Task",
      "",
      task.title,
      "",
      "## Requirements",
      ...task.requirements.map((requirement) => `- ${requirement}`),
      "",
      "## Allowed Files",
      ...task.allowedFiles.map((file) => `- ${file}`),
      "",
      "## Forbidden Files",
      ...task.forbiddenFiles.map((file) => `- ${file}`),
      "",
      "## Relevant Code Context",
      "",
      input.codebaseSummary,
      "",
      "## Verification",
      ...task.verification.map((command) => `- ${command}`),
      "",
      "## Risk",
      "",
      "Do not expand file scope or bypass existing security and architecture boundaries.",
    ].join("\n");
    return {
      tasks: [task],
      tasksMarkdown,
      testPlan: [
        "# Test Plan",
        "",
        "## Unit",
        "- Test each requirement and error branch in isolation.",
        "",
        "## Integration",
        "- Test module boundaries and persistence behavior.",
        "",
        "## End-to-End",
        "- Exercise acceptance criteria from the public interface.",
        "",
        "## Security and Concurrency",
        "- Verify authorization, invalid inputs, duplicate operations, and concurrent operations.",
      ].join("\n"),
      context,
      contextPacks: { [task.id]: contextPack },
    };
  }
}

function extractRequirements(spec: string): string[] {
  return [...spec.matchAll(/###\s+(REQ-\d+)/g)]
    .map((match) => match[1])
    .filter(Boolean) as string[];
}
