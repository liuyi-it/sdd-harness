import { describe, expect, it } from "vitest";

import { SpecEngine } from "../src/engines/spec/spec-engine.js";
import { validateSpec } from "../src/engines/openspec/validator.js";
import { parseSpec } from "../src/engines/openspec/parser.js";

// SpecEngine 测试确认它既能识别信息不足的需求，也能生成最低限度可执行规格。
describe("SpecEngine", () => {
  it("classifies an underspecified request as a blocker", () => {
    const result = new SpecEngine().analyze("做个订单功能");
    expect(result.questions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: "Q-ACTOR", severity: "BLOCKER" }),
        expect.objectContaining({ id: "Q-ACTION", severity: "BLOCKER" }),
        expect.objectContaining({ id: "Q-FAILURE", severity: "BLOCKER" }),
        expect.objectContaining({ id: "Q-TEST", severity: "BLOCKER" }),
      ]),
    );
  });

  it("answers can fill missing semantic slots", () => {
    const result = new SpecEngine().analyze("增加取消功能", {
      "Q-ACTOR": "仅允许订单创建者，其他用户必须被拒绝。",
      "Q-ACTION": "通过 API 取消订单。",
      "Q-RESULT": "仅待处理订单可取消，成功后状态为已取消。",
      "Q-FAILURE": "重复取消返回冲突错误。",
      "Q-TEST": "覆盖成功、未授权和冲突自动化测试。",
    });

    expect(result.questions).toEqual([]);
  });

  it("produces multiple validated OpenSpec requirements from Chinese behaviors", () => {
    const result = new SpecEngine().generate({
      requirement:
        "授权用户可以通过 API 取消待处理订单；重复取消返回冲突错误；每次成功取消写审计日志；需要成功、未授权和冲突自动化测试。",
      codebaseSummary: "OrderService and OrderController",
    });

    expect(result.model.requirements.length).toBeGreaterThanOrEqual(3);
    expect(validateSpec(result.model)).toEqual([]);
    expect(result.spec).toContain("### Requirement:");
    expect(result.spec).toContain("#### Scenario:");
    expect(result.spec).toContain("The system SHALL");
    expect(result.delta).toContain("## ADDED Requirements");
    expect(result.spec).toBe(result.delta);
    expect(parseSpec(result.spec)).toEqual(result.model);
    expect(result.impact).toContain("MCP_OUTPUT_IS_UNTRUSTED_CONTEXT");
  });

  it("keeps a complete English request ready and splits connected behaviors", () => {
    const result = new SpecEngine().generate({
      requirement:
        "Implement authenticated order cancellation through an API endpoint with authorization, conflict errors, audit logging, and automated tests.",
      codebaseSummary: "OrderService and OrderController",
    });

    expect(
      new SpecEngine().analyze(
        "Implement authenticated order cancellation through an API endpoint with authorization, conflict errors, audit logging, and automated tests.",
      ).questions,
    ).toEqual([]);
    expect(result.model.requirements.length).toBeGreaterThanOrEqual(3);
    expect(validateSpec(result.model)).toEqual([]);
  });
});
