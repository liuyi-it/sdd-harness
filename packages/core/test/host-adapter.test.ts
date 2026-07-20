import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  HostAdapter,
  parseHostCommand,
  type CommandResult,
  type SddCore,
} from "../src/index.js";

const roots: string[] = [];

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("HostAdapter", () => {
  it("默认协作模式只展示用户需要回答的澄清问题", async () => {
    const core = stubCore({
      ok: true,
      state: "CLARIFYING",
      exitCode: 0,
      changeId: "add-order-cancel",
      next: "sdd new",
      data: {
        clarification: {
          questions: [{ id: "Q-ACTOR", question: "请明确谁可以取消订单。" }],
        },
      },
    });

    const result = await new HostAdapter(core, "codex").execute(
      'sdd new "增加取消功能"',
      "/project",
    );

    expect(result.rendered?.content).toBe(
      "我需要先确认以下信息：\n- 请明确谁可以取消订单。\n",
    );
    expect(result.rendered?.content).not.toContain("CLARIFYING");
    expect(result.rendered?.content).not.toContain("Q-ACTOR");
    expect(result.rendered?.content).not.toContain("sdd new");
  });

  it("默认协作模式不暴露任务协议和内部路径", async () => {
    const core = stubCore({
      ok: true,
      state: "BUILD_WAITING_AGENT",
      exitCode: 0,
      next: "sdd build complete",
      actionRequired: {
        type: "AGENT_TASK_EXECUTION",
        taskId: "TASK-001",
        changeId: "add-order-cancel",
        contextPack: ".sdd/context-packs/add-order-cancel/TASK-001.md",
        allowedFiles: ["src/order.ts"],
        expectedNewFiles: [],
        forbiddenFiles: [],
        verification: [],
        resultFile: ".sdd/runs/run-1/tasks/TASK-001.result.json",
        codebase: { provider: "fallback-file-scan", degraded: false },
      },
    });

    const result = await new HostAdapter(core, "codex").execute(
      "sdd build next",
      "/project",
    );

    expect(core.execute).toHaveBeenCalledWith({
      command: "build",
      cwd: "/project",
      args: { subcommand: "next" },
    });
    expect(result.rendered?.content).toBe(
      "已理解并推进：正在按既定范围实施并验证。\n",
    );
  });

  it("能够解析并内部传递澄清答案与 auto 控制参数", () => {
    expect(
      parseHostCommand(
        `sdd new --answers '{"Q-ACTOR":"管理员"}'`,
        "/project",
        "codex",
      ).args,
    ).toEqual({ answers: { "Q-ACTOR": "管理员" } });
    expect(
      parseHostCommand("sdd auto --resume --loop-status", "/project", "codex")
        .args,
    ).toEqual({ resume: true, loopStatus: true });
  });

  it("在内部读取 build complete 的结果文件后再交给 Core", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-host-adapter-"));
    roots.push(root);
    await writeFile(join(root, "result.json"), '{"status":"DONE"}\n', "utf8");
    const core = stubCore({ ok: true, state: "BUILD_READY", exitCode: 0 });

    await new HostAdapter(core, "codex").execute(
      "sdd build complete --task TASK-001 --result result.json",
      root,
    );

    expect(core.execute).toHaveBeenCalledWith({
      command: "build",
      cwd: root,
      args: {
        subcommand: "complete",
        taskId: "TASK-001",
        result: { status: "DONE" },
      },
    });
  });

  it("严格审计模式保留现有协议文本", async () => {
    const core = stubCore({
      ok: true,
      state: "SPEC_READY",
      exitCode: 0,
      changeId: "add-order-cancel",
      next: "sdd design",
    });

    const result = await new HostAdapter(core, "codex", {
      outputMode: "strict-audit",
    }).execute('sdd new "增加取消功能"', "/project");

    expect(result.rendered?.content).toContain("SDD Status: SPEC_READY");
    expect(result.rendered?.content).toContain("add-order-cancel");
  });
});

function stubCore(
  result: CommandResult,
): SddCore & { execute: ReturnType<typeof vi.fn> } {
  return { execute: vi.fn(async () => result) };
}
