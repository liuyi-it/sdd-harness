import type { SpecDocument, SpecRequirement } from "../openspec/model.js";
import { renderSpec } from "../openspec/renderer.js";
import { validateSpec } from "../openspec/validator.js";

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
  delta: string;
  model: SpecDocument;
}

type MaybePromise<T> = T | Promise<T>;

const SEMANTIC_SLOTS: ReadonlyArray<{
  id: string;
  pattern: RegExp;
  question: string;
}> = [
  {
    id: "Q-ACTOR",
    pattern:
      /\b(authenticated|authorized|authorization|permission|user|creator|owner)\b|授权|鉴权|权限|用户|创建者|所有者/i,
    question: "谁可以执行该行为，未授权请求应如何处理？",
  },
  {
    id: "Q-ACTION",
    pattern:
      /\b(api|endpoint|request|cancel|create|update|delete|return|respond)\b|接口|请求|取消|创建|更新|删除|返回/i,
    question: "请明确要执行的动作及 API/接口行为。",
  },
  {
    id: "Q-RESULT",
    pattern:
      /\b(pending|precondition|result|success|successful|state|cancel(?:led|lation)?)\b|待处理|未完成|前置|结果|成功|状态|取消/i,
    question: "请明确业务前置条件和成功结果。",
  },
  {
    id: "Q-FAILURE",
    pattern:
      /\b(fail|failure|error|conflict|reject|denied|unauthorized|forbidden|authorization)\b|失败|错误|异常|冲突|拒绝|未授权|无权限|仅允许/i,
    question: "请明确失败、未授权或冲突时的可观察行为。",
  },
  {
    id: "Q-TEST",
    pattern: /\b(test|tests|testing|automated|automation)\b|测试|自动化|验收/i,
    question: "请明确需要覆盖的自动化测试意图。",
  },
];

export class SpecEngine {
  analyze(
    requirement: string,
    answers: Record<string, string> = {},
  ): SpecAnalysis {
    const context = [requirement, ...Object.values(answers)].join("\n");
    return {
      questions: SEMANTIC_SLOTS.filter(
        (slot) => !slot.pattern.test(context),
      ).map((slot) => ({
        id: slot.id,
        severity: "BLOCKER" as const,
        question: slot.question,
      })),
    };
  }

  generate(input: GenerateSpecInput): MaybePromise<SpecArtifacts> {
    const answers = input.answers ?? {};
    const effectiveRequirement = [input.requirement, ...Object.values(answers)]
      .filter((value) => value.trim() !== "")
      .join("；");
    const analysis = this.analyze(input.requirement, answers);
    const model = buildModel(effectiveRequirement);
    const failures = validateSpec(model);
    if (failures.length > 0) {
      throw new Error(
        `生成的 OpenSpec 无效：${failures.map((failure) => failure.message).join("；")}`,
      );
    }
    const spec = renderSpec(model);
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
      spec,
      delta: spec,
      model,
    };
  }
}

function buildModel(requirement: string): SpecDocument {
  const behaviors = splitBehaviors(requirement);
  return {
    title: "Requested Change",
    requirements: behaviors.map((behavior, index) =>
      buildRequirement(behavior, index),
    ),
  };
}

function splitBehaviors(requirement: string): string[] {
  return requirement
    .replace(
      /\s+(?:and|以及|并且|同时)\s+(?=(?:automated )?tests?\b|audit\b|conflict\b|unauthorized\b|审计|冲突|测试)/gi,
      "；",
    )
    .split(
      /[；;。\n]+|,\s+(?=(?:and\s+)?(?:automated )?tests?\b|(?:and\s+)?audit\b|(?:and\s+)?conflict\b)/i,
    )
    .map((behavior) =>
      behavior.replace(/^(?:and|以及|并且|同时)\s+/i, "").trim(),
    )
    .filter(Boolean);
}

function buildRequirement(behavior: string, index: number): SpecRequirement {
  const number = String(index + 1).padStart(3, "0");
  const kind = classifyBehavior(behavior);
  const details = scenarioFor(kind, behavior);
  return {
    id: `REQ-${number}`,
    title: titleFor(kind, index),
    statement: `The system SHALL ${normalizeStatement(behavior)}.`,
    operation: "ADDED",
    scenarios: [
      {
        id: `REQ-${number}-SC-001`,
        title: details.title,
        given: [details.given],
        when: [details.when],
        then: [details.then],
      },
    ],
  };
}

type BehaviorKind = "success" | "rejection" | "audit" | "test";

function classifyBehavior(behavior: string): BehaviorKind {
  if (/audit|审计|日志/i.test(behavior)) return "audit";
  if (/test|测试|自动化/i.test(behavior)) return "test";
  if (
    /conflict|error|fail|reject|unauthorized|forbidden|冲突|错误|失败|拒绝|未授权/i.test(
      behavior,
    )
  )
    return "rejection";
  return "success";
}

function titleFor(kind: BehaviorKind, index: number): string {
  const titles: Record<BehaviorKind, string> = {
    success: "Successful API behavior",
    rejection: "Rejected or conflicting request",
    audit: "Successful operation audit",
    test: "Automated behavior verification",
  };
  return `${titles[kind]} ${index + 1}`;
}

function scenarioFor(
  kind: BehaviorKind,
  behavior: string,
): { title: string; given: string; when: string; then: string } {
  switch (kind) {
    case "rejection":
      return {
        title: "Request is rejected",
        given: "the request violates the described authorization or state rule",
        when: "the client performs the described API operation",
        then: behavior,
      };
    case "audit":
      return {
        title: "Successful operation is audited",
        given: "the described operation completes successfully",
        when: "the success result is committed",
        then: behavior,
      };
    case "test":
      return {
        title: "Required behavior is covered by automation",
        given: "the success and rejection paths are available",
        when: "the automated verification suite runs",
        then: behavior,
      };
    case "success":
      return {
        title: "Authorized request succeeds",
        given: "the described actor and business preconditions are satisfied",
        when: behavior,
        then: "the requested business result is returned by the API",
      };
  }
}

function normalizeStatement(behavior: string): string {
  return behavior
    .replace(/^implement\s+/i, "implement ")
    .replace(/[.,，。]+$/g, "")
    .trim();
}
