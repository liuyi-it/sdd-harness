import { describe, expect, it } from "vitest";

import { SpecEngine } from "../src/engines/spec/spec-engine.js";

// SpecEngine 测试确认它既能识别信息不足的需求，也能生成最低限度可执行规格。
describe("SpecEngine", () => {
  it("classifies an underspecified request as a blocker", () => {
    const result = new SpecEngine().analyze("做个订单功能");
    expect(result.questions[0]).toMatchObject({ severity: "BLOCKER" });
  });

  it("produces a validated requirement and acceptance criteria", () => {
    const result = new SpecEngine().generate({
      requirement:
        "Implement authenticated order cancellation for pending orders with conflict handling, audit logging, and automated tests.",
      codebaseSummary: "OrderService and OrderController",
    });
    expect(result.spec).toContain("### REQ-001");
    expect(result.spec).toContain("Given");
    expect(result.spec).toContain("When");
    expect(result.spec).toContain("Then");
    expect(result.impact).toContain("MCP_OUTPUT_IS_UNTRUSTED_CONTEXT");
  });
});
