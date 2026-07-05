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
      /\b(authenticated|authorized user|user|creator|owner|actor)\b|用户|创建者|所有者|操作者/i,
    question: "请明确谁是执行该行为的业务角色。",
  },
  {
    id: "Q-AUTHORIZATION",
    pattern:
      /\b(authorization|authorized|permission|unauthorized|forbidden)\b|授权|鉴权|权限|未授权|无权限|仅允许/i,
    question: "请明确授权规则以及未授权请求的处理方式。",
  },
  {
    id: "Q-ACTION",
    pattern:
      /\b(cancel|cancellation|create|update|delete|return|respond)\b|取消|创建|更新|删除|返回/i,
    question: "请明确要执行的业务动作。",
  },
  {
    id: "Q-INTERFACE",
    pattern:
      /\b(api|endpoint|request|GET|POST|PUT|PATCH|DELETE)\b|接口|API|请求/i,
    question: "请明确承载该动作的 API 或接口行为。",
  },
  {
    id: "Q-PRECONDITION",
    pattern:
      /\b(pending|precondition|eligible|authenticated)\b|待处理|未完成|前置|满足条件/i,
    question: "请明确业务前置条件，例如允许操作的资源状态。",
  },
  {
    id: "Q-RESULT",
    pattern:
      /\b(result|success|successful|cancelled|canceled|cancellation)\b|结果|成功|已取消|取消成功|取消(?:未完成|待处理)订单/i,
    question: "请明确成功后的业务结果。",
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
      return isChinese(behavior)
        ? {
            title: "重复取消返回冲突",
            given: "订单已取消",
            when: "再次请求取消订单",
            then: "返回冲突错误",
          }
        : {
            title: "Repeated cancellation returns a conflict",
            given: "the order is already cancelled",
            when: "the authenticated client repeats the API cancellation request",
            then: "the API returns a conflict error",
          };
    case "audit":
      return isChinese(behavior)
        ? {
            title: "成功取消写入审计",
            given: "订单取消成功",
            when: "系统写入审计日志",
            then: "产生可追踪的审计记录",
          }
        : {
            title: "Successful cancellation is audited",
            given: "the authenticated order cancellation succeeds",
            when: "the system writes the cancellation audit log",
            then: "a traceable audit record is stored",
          };
    case "test":
      return isChinese(behavior)
        ? {
            title: "自动化验证取消行为",
            given: "成功、未授权和冲突场景均已定义",
            when: "运行订单取消自动化测试",
            then: "三个场景均得到断言和验证",
          }
        : {
            title: "Cancellation behavior is automated",
            given: "success, unauthorized, and conflict cases are defined",
            when: "the automated order cancellation tests run",
            then: "the API outcomes for every case are asserted",
          };
    case "success":
      return isChinese(behavior)
        ? {
            title: "授权用户取消待处理订单",
            given: "授权用户和待处理订单",
            when: "授权用户通过 API 请求取消订单",
            then: "订单被取消",
          }
        : {
            title: "Authorized actor cancels a pending order",
            given:
              "an authenticated actor with authorization and a pending order",
            when: "the actor sends the API cancellation request",
            then: "the order is cancelled",
          };
  }
}

function isChinese(value: string): boolean {
  return /[\u3400-\u9fff]/.test(value);
}

function normalizeStatement(behavior: string): string {
  return behavior
    .replace(/^implement\s+/i, "implement ")
    .replace(/[.,，。]+$/g, "")
    .trim();
}
