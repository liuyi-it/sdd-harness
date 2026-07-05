import { describe, expect, it } from "vitest";

import { scopePatternsOverlap } from "../src/security/scope-overlap.js";

describe("任务范围重叠", () => {
  it.each([
    ["src/order.ts", "src/order.ts"],
    ["src/orders/**", "src/orders/order.ts"],
    ["src/**", "src/orders/**"],
    ["src\\orders\\**", "src/orders/order.ts"],
  ])("识别 %s 与 %s 重叠", (left, right) => {
    expect(scopePatternsOverlap([left], [right])).toBe(true);
    expect(scopePatternsOverlap([right], [left])).toBe(true);
  });

  it.each([
    ["src/order.ts", "src/audit.ts"],
    ["src/orders/**", "src/audit/order.ts"],
    ["src/orders.ts", "src/orders/order.ts"],
  ])("识别 %s 与 %s 不重叠", (left, right) => {
    expect(scopePatternsOverlap([left], [right])).toBe(false);
  });
});
