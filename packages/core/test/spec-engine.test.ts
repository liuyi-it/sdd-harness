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

  it("requires each compound semantic slot independently", () => {
    const engine = new SpecEngine();
    const incomplete = engine.analyze("用户取消订单，失败返回错误，需要测试");

    expect(incomplete.questions).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: "Q-AUTHORIZATION" }),
        expect.objectContaining({ id: "Q-INTERFACE" }),
        expect.objectContaining({ id: "Q-PRECONDITION" }),
        expect.objectContaining({ id: "Q-RESULT" }),
      ]),
    );
    expect(
      engine.analyze("用户取消订单，失败返回错误，需要测试", {
        "Q-AUTHORIZATION": "仅授权用户可以操作，未授权用户被拒绝",
        "Q-INTERFACE": "通过 API 请求取消",
        "Q-PRECONDITION": "订单必须处于待处理状态",
        "Q-RESULT": "成功后订单状态变为已取消",
      }).questions,
    ).toEqual([]);
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
    expect(result.spec).toContain("GIVEN 授权用户和待处理订单");
    expect(result.spec).toContain("WHEN 授权用户通过 API 请求取消订单");
    expect(result.spec).toContain("THEN 订单被取消");
    expect(result.spec).toContain("GIVEN 订单已取消");
    expect(result.spec).toContain("WHEN 再次请求取消订单");
    expect(result.spec).toContain("THEN 返回冲突错误");
    expect(result.spec).toContain("GIVEN 订单取消成功");
    expect(result.spec).toContain("WHEN 系统写入审计日志");
    expect(result.spec).toContain("THEN 产生可追踪的审计记录");
    expect(result.spec).not.toMatch(/described|requested business result/i);
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
    expect(result.spec).toMatch(/GIVEN .*authenticated|GIVEN .*authorized/i);
    expect(result.spec).toMatch(/WHEN .*API.*cancel/i);
    expect(result.spec).toMatch(/THEN .*conflict/i);
    expect(result.spec).toMatch(/THEN .*audit/i);
    expect(result.spec).not.toMatch(/described|requested business result/i);
  });
});
