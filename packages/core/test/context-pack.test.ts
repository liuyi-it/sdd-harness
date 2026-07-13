import { describe, expect, it } from "vitest";

import {
  readContextPackMetadata,
  renderContextPack,
  verifyContextPackDigest,
} from "../src/build/context-pack.js";

const input = () => ({
  body: "# 当前任务\n\n只实现当前纵向切片。",
  rules: {
    host: "codex" as const,
    sources: [],
    acknowledgement: "MUST_FOLLOW_PROJECT_RULES" as const,
    hash: `sha256:${"1".repeat(64)}`,
  },
  codebaseSummary: "不应复制的代码库全文",
  spec: "不应复制的规格全文",
  design: "不应复制的设计全文",
  impact: "不应复制的影响分析全文",
  tasksMarkdown: "# Tasks",
  tasksJson: "[]",
  projectConventionsHash: `sha256:${"2".repeat(64)}`,
  references: {
    spec: ".sdd/changes/change/spec.md",
    design: ".sdd/changes/change/design.md",
    plan: ".sdd/changes/change/tasks.md",
    impact: ".sdd/changes/change/impact.md",
    codebase: ".sdd/index/codebase-summary.md",
  },
  task: {
    taskId: "TASK-001",
    objective: "交付取消订单能力",
    userVisibleOutcome: "用户可取消待处理订单",
    requiredFiles: ["src/order.ts"],
    allowedFiles: ["src/order.ts", "test/order.test.ts"],
    forbiddenFiles: [".env"],
    verification: ["npm test"],
  },
});

describe("Context Pack v2", () => {
  it("使用仓库内引用且不复制已有制品全文", () => {
    const content = renderContextPack(input());
    const metadata = readContextPackMetadata(content);

    expect(metadata.schemaVersion).toBe("2.0.0");
    expect(metadata.contextPackDigest).toMatch(/^sha256:[a-f0-9]{64}$/u);
    expect(content).toContain("spec: .sdd/changes/change/spec.md");
    expect(content).toContain("Policy Refs");
    expect(content).not.toContain("不应复制的规格全文");
    expect(content).not.toContain("不应复制的设计全文");
    expect(content).not.toContain("不应复制的代码库全文");
    expect(verifyContextPackDigest(content)).toBe(true);
  });

  it("篡改 allowedFiles 后摘要校验失败", () => {
    const content = renderContextPack(input());
    const tampered = content.replace("- src/order.ts", "- **");
    expect(verifyContextPackDigest(tampered)).toBe(false);
  });

  it.each(["../secret.md", "/tmp/secret.md", "docs/../../secret.md"])(
    "拒绝越出仓库的引用 %s",
    (path) => {
      const value = input();
      value.references.spec = path;
      expect(() => renderContextPack(value)).toThrow(
        "Context Pack 引用必须是仓库内相对路径",
      );
    },
  );
});
