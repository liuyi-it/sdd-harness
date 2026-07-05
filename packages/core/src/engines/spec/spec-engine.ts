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
      /\b(authenticated|authorized (?:user|administrator|actor)|user|administrator|creator|owner|actor)\b|用户|管理员|创建者|所有者|操作者/i,
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
      /\b(pending|precondition|eligible|authenticated|unregistered)\b|待处理|未完成|未注册|前置|满足条件/i,
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
    const effectiveRequirement = composeEffectiveRequirement(
      input.requirement,
      answers,
    );
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

function composeEffectiveRequirement(
  requirement: string,
  answers: Record<string, string>,
): string {
  if (Object.keys(answers).length === 0) return requirement;
  const primaryIds = [
    "Q-ACTOR",
    "Q-AUTHORIZATION",
    "Q-ACTION",
    "Q-INTERFACE",
    "Q-PRECONDITION",
    "Q-RESULT",
  ];
  const primary = [requirement, ...primaryIds.map((id) => answers[id])]
    .filter((value): value is string => Boolean(value?.trim()))
    .join("，");
  const hasStructuredAnswers = primaryIds.some((id) => answers[id]);
  if (!hasStructuredAnswers)
    return [requirement, ...Object.values(answers)].join("，");
  const remaining = Object.entries(answers)
    .filter(([id]) => !primaryIds.includes(id))
    .map(([, value]) => value)
    .filter((value) => value.trim() !== "");
  return [primary, ...remaining].join("；");
}

function buildModel(requirement: string): SpecDocument {
  const behaviors = splitBehaviors(requirement);
  return {
    title: "Requested Change",
    requirements: behaviors.map((behavior, index) =>
      buildRequirement(behavior, index, requirement),
    ),
  };
}

function splitBehaviors(requirement: string): string[] {
  return requirement
    .replace(
      /\s+(?:and|以及|并且|同时)\s+(?=(?:automated )?tests?\b|audit\b|审计|测试)/gi,
      "；",
    )
    .replace(/,\s+(?=(?:conflict\s+(?:error|handling)|audit\b))/gi, "；")
    .split(
      /[；;。\n]+|,\s+(?=(?:and\s+)?(?:automated )?tests?\b|(?:and\s+)?audit\b|(?:and\s+)?conflict\s+(?:error|handling))/i,
    )
    .map((behavior) =>
      behavior.replace(/^(?:and|以及|并且|同时)\s+/i, "").trim(),
    )
    .filter(Boolean);
}

function buildRequirement(
  behavior: string,
  index: number,
  context: string,
): SpecRequirement {
  const number = String(index + 1).padStart(3, "0");
  const kind = classifyBehavior(behavior);
  const details = scenarioFor(kind, behavior, context);
  assertConcreteScenario(details, behavior);
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
  if (
    /\b(?:api|endpoint|POST|PUT|PATCH|DELETE)\b|接口/i.test(behavior) &&
    /\b(?:cancel|cancellation|create|update|delete)\b|取消|创建|更新|删除/i.test(
      behavior,
    )
  )
    return "success";
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
  context: string,
): { title: string; given: string; when: string; then: string } {
  const chinese = isChinese(behavior);
  switch (kind) {
    case "rejection":
      return rejectionScenario(behavior, context, chinese);
    case "audit":
      return auditScenario(behavior, context, chinese);
    case "test":
      return chinese
        ? {
            title: "自动化验证需求行为",
            given: extractTestCases(behavior),
            when: `运行${extractAction(context, true)}自动化测试`,
            then: `${extractTestCases(behavior)}均得到断言和验证`,
          }
        : {
            title: "Required behavior is automated",
            given: extractTestCases(behavior),
            when: `the automated ${extractAction(context, false)} tests run`,
            then: `${extractTestCases(behavior)} are asserted`,
          };
    case "success":
      return chinese
        ? {
            title: `${extractActor(context, true)}执行成功行为`,
            given: `${extractActor(context, true)}和${extractPrecondition(context, true)}`,
            when: `${extractActor(context, true)}通过 API 请求${extractAction(behavior, true)}`,
            then: extractResult(behavior, true),
          }
        : {
            title: `${extractActor(context, false)} completes the action`,
            given: `${extractActor(context, false)} and ${extractPrecondition(context, false)}`,
            when: `${extractActor(context, false)} sends the API request to ${extractAction(behavior, false)}`,
            then: extractResult(behavior, false),
          };
  }
}

function rejectionScenario(
  behavior: string,
  context: string,
  chinese: boolean,
): { title: string; given: string; when: string; then: string } {
  if (!chinese && !/\b(?:returns?|becomes?)\b/i.test(behavior)) {
    const action = extractAction(context, false);
    return {
      title: `${action} conflict is rejected`,
      given: `${action} has already completed for the target resource`,
      when: `the client repeats the API request to ${action}`,
      then: `the API returns ${behavior}`,
    };
  }
  const when = beforeResultMarker(behavior);
  const then = afterResultMarker(behavior);
  if (chinese) {
    const subject = when.replace(/^重复/, "").replace(/(?:创建|取消)$/, "");
    const action = extractAction(context, true);
    const resource = action.replace(/^(?:创建|取消|更新|删除)/, "");
    const operation = action.slice(0, action.length - resource.length);
    return {
      title: `${when}被拒绝`,
      given: /取消/.test(when)
        ? `${subject || resource}已${operation}`
        : `${subject || resource}已存在`,
      when: /重复取消/.test(when) ? `再次请求${action}` : when,
      then,
    };
  }
  const subject = when.replace(/^(?:duplicate|repeated)\s+/i, "");
  return {
    title: `${when} is rejected`,
    given: `${subject} already exists or has already completed`,
    when,
    then,
  };
}

function auditScenario(
  behavior: string,
  context: string,
  chinese: boolean,
): { title: string; given: string; when: string; then: string } {
  const writeIndex = behavior.search(/写|writes?\b/i);
  if (writeIndex < 0 && !/\baudit logging\b/i.test(behavior))
    throw specificationError(behavior, "缺少审计写入动作");
  const action = extractAction(context, chinese);
  const successful =
    writeIndex > 0
      ? behavior.slice(0, writeIndex).replace(/^每次/, "").trim()
      : chinese
        ? `${action}成功`
        : `${action} succeeds`;
  const written =
    writeIndex >= 0
      ? behavior.slice(writeIndex).trim()
      : chinese
        ? "写审计日志"
        : "writes an audit log";
  return chinese
    ? {
        title: `${successful}写入审计`,
        given: action.startsWith("取消")
          ? `${action.replace(/^取消/, "")}${action.slice(0, 2)}成功`
          : successful,
        when: `系统${written.replace(/^写/, "写入")}`,
        then: written.includes("审计日志")
          ? "产生可追踪的审计记录"
          : `${written}被保存`,
      }
    : {
        title: `${successful} is audited`,
        given: successful,
        when: `the system ${written}`,
        then: `${written.replace(/^writes?\s+/i, "")} is stored as a traceable record`,
      };
}

function extractActor(context: string, chinese: boolean): string {
  const match = chinese
    ? /(授权(?:用户|管理员|创建者|所有者)|(?:用户|管理员|创建者|所有者))/i.exec(
        context,
      )
    : /\b((?:an?\s+)?(?:authenticated|authorized)(?:\s+and\s+(?:authenticated|authorized))?\s+(?:user|administrator|actor|creator|owner)|(?:an?\s+)?(?:user|administrator|actor|creator|owner))\b/i.exec(
        context,
      );
  if (!match?.[1] && !chinese && /\bauthenticated\b/i.test(context))
    return "an authenticated actor with authorization";
  if (!match?.[1]) throw specificationError(context, "缺少具体 actor");
  return match[1].trim();
}

function extractPrecondition(context: string, chinese: boolean): string {
  const match = chinese
    ? /((?:邮箱)?未注册|待处理订单|未完成订单|[^，；,;]{1,20}满足条件)/i.exec(
        context,
      )
    : /((?:the\s+)?email\s+is\s+unregistered|(?:an?\s+)?pending\s+order|[^,;]{1,40}\s+is\s+eligible)/i.exec(
        context,
      );
  if (!match?.[1] && !chinese && /\bauthenticated\b/i.test(context))
    return "authentication is satisfied";
  if (!match?.[1]) throw specificationError(context, "缺少具体前置条件");
  return match[1].trim();
}

function extractAction(context: string, chinese: boolean): string {
  const match = chinese
    ? /(创建用户|取消(?:待处理|未完成)?订单|更新[^，；,;]+|删除[^，；,;]+)/i.exec(
        context,
      )
    : /\b(create\s+(?:a\s+)?user|cancel(?:lation|\s+(?:a\s+)?(?:pending\s+)?order)?|update\s+[^,;]+|delete\s+[^,;]+)/i.exec(
        context,
      );
  if (!match?.[1]) throw specificationError(context, "缺少具体动作");
  return match[1]
    .replace(/(取消)(?:待处理|未完成)(订单)/, "$1$2")
    .replace(/^cancellation$/i, "cancel the target resource")
    .trim();
}

function extractResult(behavior: string, chinese: boolean): string {
  const marked = afterResultMarker(behavior);
  if (marked !== behavior) return marked;
  const action = extractAction(behavior, chinese);
  if (chinese && action.startsWith("取消"))
    return `${action.replace(/^取消/, "")}被${action.slice(0, 2)}`;
  if (!chinese && /cancel/i.test(action))
    return `${action.replace(/^cancel\s+(?:the\s+)?/i, "the ")} is cancelled`;
  throw specificationError(behavior, "缺少具体成功结果");
}

function beforeResultMarker(behavior: string): string {
  const marker = /返回|变为|\bbecomes?\b|\breturns?\b/i.exec(behavior);
  if (!marker || marker.index === 0)
    throw specificationError(behavior, "缺少结果前的动作");
  return behavior.slice(0, marker.index).trim();
}

function afterResultMarker(behavior: string): string {
  const marker = /返回|变为|\bbecomes?\b|\breturns?\b/i.exec(behavior);
  return marker ? behavior.slice(marker.index).trim() : behavior;
}

function extractTestCases(behavior: string): string {
  const cases = behavior
    .replace(/^(?:需要|automated tests cover)\s*/i, "")
    .replace(/(?:自动化)?测试.*$/i, "")
    .replace(/cases?\.?$/i, "")
    .trim();
  if (!cases) throw specificationError(behavior, "缺少具体测试场景");
  return cases;
}

function assertConcreteScenario(
  scenario: { given: string; when: string; then: string },
  behavior: string,
): void {
  const values = [scenario.given, scenario.when, scenario.then].map((value) =>
    value.trim().toLowerCase(),
  );
  if (values.some((value) => value === "") || new Set(values).size !== 3) {
    throw specificationError(behavior, "GIVEN/WHEN/THEN 必须非空且互异");
  }
}

function specificationError(behavior: string, reason: string): Error {
  return new Error(`无法从行为“${behavior}”生成具体 Scenario：${reason}`);
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
