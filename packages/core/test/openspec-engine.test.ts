import { describe, expect, it } from "vitest";

import {
  extractRequirementIds,
  parseSpec,
  renderSpec,
  validateSpec,
  type SpecDocument,
} from "../src/index.js";

describe("OpenSpec 结构化规格引擎", () => {
  it("从 mixed-format 规格提取全部 requirement ID", () => {
    expect(
      extractRequirementIds(
        "### REQ-007: Legacy\n\n### Requirement: New behavior\n\n### REQ-009: Legacy two\n\n### Requirement: Another behavior",
      ),
    ).toEqual(["REQ-007", "REQ-001", "REQ-009", "REQ-002"]);
  });

  it("解析规范示例并按出现顺序生成稳定 ID", () => {
    const document = parseSpec(`# 账户规格

## ADDED Requirements

### Requirement: 用户登录
系统 SHALL 允许有效用户登录。

#### Scenario: 登录成功
- GIVEN 用户已注册
- GIVEN 账户未锁定
- WHEN 用户提交正确凭据
- THEN 系统创建会话
- THEN 系统返回首页

### Requirement: 登录审计
The system MUST record successful logins.

#### Scenario: 记录审计事件
- WHEN 登录成功
- THEN 写入审计日志
`);

    expect(document).toEqual({
      title: "账户规格",
      requirements: [
        {
          id: "REQ-001",
          title: "用户登录",
          statement: "系统 SHALL 允许有效用户登录。",
          operation: "ADDED",
          scenarios: [
            {
              id: "REQ-001-SC-001",
              title: "登录成功",
              given: ["用户已注册", "账户未锁定"],
              when: ["用户提交正确凭据"],
              then: ["系统创建会话", "系统返回首页"],
            },
          ],
        },
        {
          id: "REQ-002",
          title: "登录审计",
          statement: "The system MUST record successful logins.",
          operation: "ADDED",
          scenarios: [
            {
              id: "REQ-002-SC-001",
              title: "记录审计事件",
              given: [],
              when: ["登录成功"],
              then: ["写入审计日志"],
            },
          ],
        },
      ],
    });
  });

  it("报告缺少规范关键字的 requirement", () => {
    const failures = validateSpec({
      title: "关键词校验",
      requirements: [requirement({ statement: "系统允许用户登录。" })],
    });

    expect(failures).toContainEqual(
      expect.objectContaining({
        code: "SPEC_NORMATIVE_KEYWORD_REQUIRED",
        path: "requirements[0].statement",
      }),
    );
  });

  it("报告缺少 Scenario 的 requirement", () => {
    const failures = validateSpec({
      title: "场景校验",
      requirements: [requirement({ scenarios: [] })],
    });

    expect(failures).toContainEqual(
      expect.objectContaining({
        code: "SPEC_SCENARIO_REQUIRED",
        path: "requirements[0].scenarios",
      }),
    );
  });

  it("报告 requirement 与 scenario 的重复 ID", () => {
    const failures = validateSpec({
      title: "ID 校验",
      requirements: [
        requirement(),
        requirement({
          id: "REQ-001",
          title: "第二项",
          scenarios: [scenario({ id: "REQ-001-SC-001", title: "第二场景" })],
        }),
      ],
    });

    expect(
      failures.filter(({ code }) => code === "SPEC_DUPLICATE_ID"),
    ).toHaveLength(2);
  });

  it("报告同标题互斥 delta 操作", () => {
    const failures = validateSpec({
      title: "Delta 校验",
      requirements: [
        requirement({ operation: "ADDED" }),
        requirement({ id: "REQ-002", operation: "REMOVED" }),
      ],
    });

    expect(failures).toContainEqual(
      expect.objectContaining({
        code: "SPEC_DELTA_CONFLICT",
        path: "requirements[1].operation",
      }),
    );
  });

  it("按去除首尾空白且忽略大小写的标题报告 delta 冲突", () => {
    const failures = validateSpec({
      title: "Delta 标题规范化",
      requirements: [
        requirement({ title: "Login", operation: "ADDED" }),
        requirement({
          id: "REQ-002",
          title: " login ",
          operation: "MODIFIED",
          scenarios: [scenario({ id: "REQ-002-SC-001" })],
        }),
        requirement({
          id: "REQ-003",
          title: "LOGIN",
          operation: "REMOVED",
          scenarios: [scenario({ id: "REQ-003-SC-001" })],
        }),
      ],
    });

    expect(
      failures.filter(({ code }) => code === "SPEC_DELTA_CONFLICT"),
    ).toHaveLength(2);
  });

  it("render 后再次 parse 保持语义稳定", () => {
    const source = `# Mixed 中英文规格

## MODIFIED Requirements

### Requirement: Session 会话
The service SHALL refresh 会话 safely.

#### Scenario: Refresh 刷新
- GIVEN token 有效
- WHEN client 请求刷新
- THEN service 返回新 token
`;
    const parsed = parseSpec(source);

    expect(parseSpec(renderSpec(parsed))).toEqual(parsed);
  });

  it.each([
    ["文档标题换行", { title: "安全标题\n## REMOVED Requirements" }],
    [
      "Requirement 标题回车",
      {
        requirements: [
          requirement({ title: "Login\r### Requirement: injected" }),
        ],
      },
    ],
    [
      "statement 注入",
      {
        requirements: [
          requirement({
            statement: "系统 SHALL 工作。\n## REMOVED Requirements",
          }),
        ],
      },
    ],
    [
      "Scenario 标题含 NUL",
      {
        requirements: [
          requirement({ scenarios: [scenario({ title: "成功\0隐藏" })] }),
        ],
      },
    ],
    [
      "GIVEN 步骤注入",
      {
        requirements: [
          requirement({
            scenarios: [scenario({ given: ["ok\n## REMOVED Requirements"] })],
          }),
        ],
      },
    ],
    [
      "WHEN 步骤注入",
      {
        requirements: [
          requirement({
            scenarios: [scenario({ when: ["执行\r- THEN 注入"] })],
          }),
        ],
      },
    ],
    [
      "THEN 步骤注入",
      {
        requirements: [
          requirement({ scenarios: [scenario({ then: ["结果\0隐藏"] })] }),
        ],
      },
    ],
  ])("render 拒绝不可安全渲染字段：%s", (_name, override) => {
    const document: SpecDocument = {
      title: "安全规格",
      requirements: [requirement()],
      ...override,
    };

    expect(() => renderSpec(document)).toThrow("不可包含 CR、LF 或 NUL");
  });

  it.each([
    "## REMOVED Requirements",
    "### Requirement: injected",
    "- THEN injected",
    "- arbitrary bullet",
  ])("render 拒绝 statement 单行结构注入：%s", (statement) => {
    const document: SpecDocument = {
      title: "安全规格",
      requirements: [requirement({ statement })],
    };

    expect(() => renderSpec(document)).toThrow(
      "statement 不可注入 Markdown 结构",
    );
  });

  it.each([
    [
      "缺失文档标题",
      "## ADDED Requirements\n\n### Requirement: A\nSystem SHALL work.",
    ],
    ["非法文档标题层级", "## 文档\n\n## ADDED Requirements"],
    ["孤立 scenario", "# 文档\n\n#### Scenario: 孤立\n- GIVEN 条件"],
    ["孤立步骤", "# 文档\n\n- THEN 结果"],
    [
      "scenario 层级错误",
      "# 文档\n\n## ADDED Requirements\n\n### Requirement: A\nSystem SHALL work.\n\n### Scenario: 错误层级",
    ],
  ])("拒绝畸形输入：%s", (_name, source) => {
    expect(() => parseSpec(source)).toThrow(Error);
  });

  it("拒绝仅含空白的 Requirement 标题", () => {
    expect(() =>
      parseSpec(`# 文档

## ADDED Requirements

### Requirement:${"   "}
`),
    ).toThrow("Requirement 标题不能为空");
  });

  it("拒绝仅含空白的 Scenario 标题", () => {
    expect(() =>
      parseSpec(`# 文档

## ADDED Requirements

### Requirement: 有效标题
系统 SHALL 工作。

#### Scenario:${"   "}
`),
    ).toThrow("Scenario 标题不能为空");
  });
});

function scenario(
  overrides: Partial<
    SpecDocument["requirements"][number]["scenarios"][number]
  > = {},
) {
  return {
    id: "REQ-001-SC-001",
    title: "成功",
    given: ["前置条件"],
    when: ["执行操作"],
    then: ["得到结果"],
    ...overrides,
  };
}

function requirement(
  overrides: Partial<SpecDocument["requirements"][number]> = {},
) {
  return {
    id: "REQ-001",
    title: "用户登录",
    statement: "系统 SHALL 允许用户登录。",
    operation: "ADDED" as const,
    scenarios: [scenario()],
    ...overrides,
  };
}
