import { describe, expect, it } from "vitest";

import { decide } from "../src/loop/loop-decision.js";

describe("Loop repair decision", () => {
  it.each(["E_VERIFY_FAILED", "E_REVIEW_FAILED"] as const)(
    "%s 已生成 REPAIR 时继续进入现有 Build 流程",
    (code) => {
      expect(
        decide({
          result: {
            ok: false,
            state: "PLAN_READY",
            exitCode: 4,
            error: {
              code,
              message: "门禁失败",
              next: "sdd build next",
            },
          },
        }),
      ).toBe("CONTINUE");
    },
  );

  it("未生成 REPAIR 的门禁失败仍暂停等待人工决策", () => {
    expect(
      decide({
        result: {
          ok: false,
          state: "PAUSED",
          exitCode: 4,
          error: {
            code: "E_VERIFY_FAILED",
            message: "修复预算已耗尽",
            next: "sdd status",
          },
        },
      }),
    ).toBe("PAUSE_FOR_HUMAN");
  });
});
