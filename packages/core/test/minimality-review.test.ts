import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { scanDeliberateDebt } from "../src/quality/deliberate-debt.js";
import { collectChangeComplexityMetrics } from "../src/quality/change-complexity.js";
import { collectDependencyDelta } from "../src/quality/dependency-delta.js";
import { runMinimalityReview } from "../src/quality/minimality-review.js";
import { createReviewReport } from "../src/quality/review-report.js";
import type { GitSnapshot } from "../src/git/git-inspector.js";

const roots: string[] = [];

function snapshot(
  files: Record<string, string>,
  manifests: Record<string, string | null>,
): GitSnapshot {
  return {
    available: true,
    files: Object.keys(files).sort(),
    hashes: files,
    tracked: ["package.json", "src/demo.ts"],
    manifests,
  };
}

afterEach(async () => {
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("minimality review", () => {
  it("确定性比较四类 package.json 依赖变化", () => {
    const before = snapshot(
      { "package.json": "before" },
      {
        "package.json": JSON.stringify({
          dependencies: { keep: "1.0.0", remove: "1.0.0" },
          devDependencies: { dev: "1.0.0" },
        }),
      },
    );
    const after = snapshot(
      { "package.json": "after" },
      {
        "package.json": JSON.stringify({
          dependencies: { keep: "2.0.0", added: "1.0.0" },
          peerDependencies: { peer: "1.0.0" },
          optionalDependencies: { optional: "1.0.0" },
        }),
      },
    );
    expect(collectDependencyDelta(before, after)).toMatchObject({
      issues: [],
      dependencies: expect.arrayContaining([
        expect.objectContaining({ name: "added", change: "ADDED" }),
        expect.objectContaining({ name: "remove", change: "REMOVED" }),
        expect.objectContaining({ name: "keep", change: "UPDATED" }),
        expect.objectContaining({ name: "peer", change: "ADDED" }),
        expect.objectContaining({ name: "optional", change: "ADDED" }),
      ]),
    });
  });

  it("未计划新增依赖阻断，已计划新增依赖允许", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-minimality-"));
    roots.push(root);
    const before = snapshot(
      { "package.json": "before" },
      { "package.json": JSON.stringify({ dependencies: {} }) },
    );
    const after = snapshot(
      { "package.json": "after" },
      { "package.json": JSON.stringify({ dependencies: { zod: "4.4.3" } }) },
    );
    const blocked = await runMinimalityReview({
      root,
      baseline: before,
      current: after,
      plannedDependencies: [],
      taskResults: [],
    });
    expect(blocked.issues).toEqual([
      expect.objectContaining({
        category: "UNPLANNED_DEPENDENCY",
        severity: "MAJOR",
      }),
    ]);
    const allowed = await runMinimalityReview({
      root,
      baseline: before,
      current: after,
      plannedDependencies: [
        {
          name: "zod",
          manifest: "package.json",
          action: "ADD",
          reason: "结构化输入校验",
          requirementIds: ["REQ-001"],
        },
      ],
      taskResults: [],
    });
    expect(allowed.issues).toEqual([]);
  });

  it("Windows manifest 路径也能匹配已计划依赖", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-windows-manifest-"));
    roots.push(root);
    const before = snapshot(
      { "packages/app/package.json": "before" },
      {
        "packages/app/package.json": JSON.stringify({ dependencies: {} }),
      },
    );
    const after = snapshot(
      { "packages/app/package.json": "after" },
      {
        "packages/app/package.json": JSON.stringify({
          dependencies: { zod: "4.4.3" },
        }),
      },
    );

    const result = await runMinimalityReview({
      root,
      baseline: before,
      current: after,
      plannedDependencies: [
        {
          name: "zod",
          manifest: "packages\\app\\package.json",
          action: "ADD",
          reason: "结构化输入校验",
          requirementIds: ["REQ-001"],
        },
      ],
      taskResults: [],
    });

    expect(result.issues).toEqual([]);
  });

  it("主版本升级阻断，删除依赖仅记录信息", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-dependency-update-"));
    roots.push(root);
    const before = snapshot(
      { "package.json": "before" },
      {
        "package.json": JSON.stringify({
          dependencies: { major: "1.0.0", removed: "1.0.0" },
        }),
      },
    );
    const after = snapshot(
      { "package.json": "after" },
      { "package.json": JSON.stringify({ dependencies: { major: "2.0.0" } }) },
    );
    const result = await runMinimalityReview({
      root,
      baseline: before,
      current: after,
      plannedDependencies: [],
      taskResults: [],
    });
    expect(result.issues).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          severity: "MAJOR",
          message: expect.stringContaining("major"),
        }),
        expect.objectContaining({
          severity: "INFO",
          message: expect.stringContaining("removed"),
        }),
      ]),
    );
  });

  it("只扫描变更文本文件中的有效 sdd-debt", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-debt-"));
    roots.push(root);
    await mkdir(join(root, "src"));
    await writeFile(
      join(root, "src/demo.ts"),
      [
        "// sdd-debt: 全局锁; trigger=等待超过 5%; upgrade=改为项目分片锁",
        "# sdd-debt: 缺少升级; trigger=出现并发",
      ].join("\r\n"),
    );
    await mkdir(join(root, "node_modules"));
    await writeFile(
      join(root, "node_modules/ignored.js"),
      "// sdd-debt: 不应扫描; trigger=x; upgrade=y\n",
    );
    const baseline = snapshot({}, {});
    const current = snapshot(
      { "src/demo.ts": "changed", "node_modules/ignored.js": "changed" },
      {},
    );
    const result = await scanDeliberateDebt(root, baseline, current);
    expect(result.debts).toEqual([
      expect.objectContaining({
        file: "src/demo.ts",
        line: 1,
        ceiling: "全局锁",
      }),
    ]);
    expect(result.issues).toEqual([
      expect.objectContaining({
        category: "DELIBERATE_DEBT",
        severity: "MINOR",
      }),
    ]);
  });

  it("清理块注释结尾并跳过二进制 debt 标记", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-debt-comments-"));
    roots.push(root);
    await mkdir(join(root, "src"));
    await writeFile(
      join(root, "src/comment.ts"),
      "/* sdd-debt: 全局锁; trigger=等待; upgrade=分片锁 */\n",
    );
    await writeFile(
      join(root, "src/binary.bin"),
      Buffer.from([0, ...Buffer.from("sdd-debt: x; trigger=y; upgrade=z")]),
    );
    const baseline = snapshot({}, {});
    const current = snapshot(
      { "src/comment.ts": "changed", "src/binary.bin": "changed" },
      {},
    );

    const result = await scanDeliberateDebt(root, baseline, current);

    expect(result.debts).toEqual([
      expect.objectContaining({ upgrade: "分片锁", file: "src/comment.ts" }),
    ]);
  });

  it("正确统计无末尾换行和带空格路径，并在 Git 不可用时返回 null", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-complexity-"));
    roots.push(root);
    execFileSync("git", ["init", "-b", "main"], { cwd: root });
    await writeFile(join(root, "space name.ts"), "first\nsecond");
    const baseline = snapshot({}, {});
    baseline.tracked = [];
    const current = snapshot({ "space name.ts": "changed" }, {});
    current.tracked = [];

    await expect(
      collectChangeComplexityMetrics(root, baseline, current),
    ).resolves.toMatchObject({
      filesAdded: 1,
      linesAdded: 2,
      linesDeleted: 0,
      netLines: 2,
    });

    const unavailable = {
      available: false,
      files: [],
      hashes: {},
      tracked: [],
    };
    await expect(
      collectChangeComplexityMetrics(root, unavailable, unavailable),
    ).resolves.toMatchObject({
      linesAdded: null,
      linesDeleted: null,
      netLines: null,
    });
  });

  it("单消费者抽象只生成 deterministic=false 的非阻断建议", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-advisory-"));
    roots.push(root);
    const unchanged = snapshot({}, {});
    const result = await runMinimalityReview({
      root,
      baseline: unchanged,
      current: unchanged,
      plannedDependencies: [],
      taskResults: [
        {
          taskId: "TASK-001",
          modifiedFiles: ["src/factory.ts"],
          tddEvidence: [],
          verification: [],
          minimality: {
            reusedExisting: [],
            standardLibraryChoices: [],
            nativePlatformChoices: [],
            dependenciesAdded: [],
            abstractionsAdded: [
              {
                name: "OrderFactory",
                file: "src/factory.ts",
                consumers: ["OrderService"],
                reason: "创建订单",
              },
            ],
            deliberateDebts: [],
          },
        },
      ],
    });

    expect(result.issues).toEqual([
      expect.objectContaining({
        category: "COMPLEXITY",
        severity: "MINOR",
        deterministic: false,
      }),
    ]);
    expect(
      createReviewReport({ changeId: "demo", issues: result.issues }).result,
    ).toBe("PASS");
  });
});
