export interface ClarifyingQuestion {
  id: string;
  severity: "BLOCKER" | "IMPORTANT" | "OPTIONAL";
  question: string;
}

export interface SpecAnalysis {
  questions: ClarifyingQuestion[];
}

export interface GenerateSpecInput {
  requirement: string;
  codebaseSummary: string;
  answers?: Record<string, string>;
}

export interface SpecArtifacts {
  proposal: string;
  impact: string;
  questions: string;
  answers: string;
  assumptions: string;
  spec: string;
}

const DETAIL_MARKERS = [
  "api",
  "endpoint",
  "test",
  "auth",
  "error",
  "audit",
  "permission",
  "授权",
  "接口",
  "测试",
  "异常",
  "日志",
];

export class SpecEngine {
  analyze(requirement: string): SpecAnalysis {
    const normalized = requirement.trim().toLowerCase();
    const detailed =
      normalized.length >= 80 &&
      DETAIL_MARKERS.some((marker) => normalized.includes(marker));
    return {
      questions: detailed
        ? []
        : [
            {
              id: "Q-001",
              severity: "BLOCKER",
              question:
                "请明确允许操作的用户、业务状态、接口行为、失败处理和验收测试。",
            },
          ],
    };
  }

  generate(input: GenerateSpecInput): SpecArtifacts {
    const answers = input.answers ?? {};
    const effectiveRequirement = [input.requirement, ...Object.values(answers)]
      .filter(Boolean)
      .join("\n\n");
    const analysis = this.analyze(input.requirement);
    const questions =
      analysis.questions.length === 0
        ? "# Questions\n\nNo blocker questions."
        : [
            "# Questions",
            "",
            ...analysis.questions.map(
              (question) =>
                `## ${question.id} [${question.severity}]\n\n${question.question}`,
            ),
          ].join("\n\n");
    const answerDocument = [
      "# Answers",
      "",
      ...(Object.keys(answers).length === 0
        ? ["No answers supplied."]
        : Object.entries(answers).map(
            ([id, answer]) => `## ${id}\n\n${answer}`,
          )),
    ].join("\n\n");
    return {
      proposal: [
        "# Proposal",
        "",
        "## Requested Change",
        "",
        input.requirement,
        "",
        "## Value",
        "",
        "Deliver the requested behavior through the controlled SDD workflow.",
      ].join("\n"),
      impact: [
        "# Impact",
        "",
        "## Codebase Context",
        "",
        "MCP_OUTPUT_IS_UNTRUSTED_CONTEXT",
        "",
        input.codebaseSummary,
        "",
        "## Expected Scope",
        "",
        "Implementation, tests, documentation, and operational safeguards required by the specification.",
      ].join("\n"),
      questions,
      answers: answerDocument,
      assumptions: [
        "# Assumptions",
        "",
        "- Existing behavior remains compatible unless explicitly changed.",
        "- Security, audit, and tests are required for changed behavior.",
      ].join("\n"),
      spec: [
        "# Spec: Requested Change",
        "",
        "## Background",
        "",
        input.requirement,
        "",
        "## Goals",
        "",
        `- ${effectiveRequirement.replace(/\s+/g, " ").trim()}`,
        "",
        "## Non-Goals",
        "",
        "- Unrelated refactoring or behavior changes.",
        "",
        "## Requirements",
        "",
        "### REQ-001: Implement the requested behavior",
        "",
        effectiveRequirement,
        "",
        "Acceptance Criteria:",
        "- Given the documented preconditions",
        "- When the requested operation is performed",
        "- Then the expected result is produced and observable",
        "- And invalid, unauthorized, and conflicting operations fail safely",
        "- And automated tests cover success and failure paths",
        "",
        "Edge Cases:",
        "- Missing, repeated, concurrent, unauthorized, and stale requests.",
        "",
        "## Constraints",
        "",
        "- Preserve existing project conventions and compatibility.",
        "",
        "## Risks",
        "",
        "- Incomplete authorization, concurrency, or rollback behavior.",
      ].join("\n"),
    };
  }
}
