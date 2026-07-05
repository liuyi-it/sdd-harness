import { describe, expect, it, vi } from "vitest";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  type CommandRequest,
  type CommandResult,
  type SddCore,
} from "../core/src/contracts.js";
import type { McpTransport } from "../core/src/codebase/codebase-adapter.js";
import type { TaskExecutor } from "../core/src/build/task-executor.js";
import { ClaudeCodeAdapter } from "../claude-code-plugin/src/adapter.js";
import { CodexAdapter } from "../codex-plugin/src/adapter.js";

// 适配器契约测试保证 Claude Code 与 Codex 只是语法不同，不改变 Core 语义。
class RecordingCore implements SddCore {
  readonly execute = vi.fn(
    async (request: CommandRequest): Promise<CommandResult> => ({
      ok: true,
      state: "INDEX_READY",
      exitCode: 0,
      data: request,
    }),
  );
}

describe("适配器契约一致性", () => {
  it.each([
    "init",
    "auto",
    "new",
    "design",
    "plan",
    "build",
    "verify",
    "review",
    "archive",
    "status",
  ] as const)("maps %s to the same Core request", async (command) => {
    const claudeCore = new RecordingCore();
    const codexCore = new RecordingCore();
    const suffix =
      command === "new" || command === "auto"
        ? ' "implement cancellation" --change add-cancel'
        : "";

    await new ClaudeCodeAdapter(claudeCore).execute(
      `/sdd.${command}${suffix}`,
      "/repo",
    );
    await new CodexAdapter(codexCore).execute(
      `sdd ${command}${suffix}`,
      "/repo",
    );

    expect(claudeCore.execute).toHaveBeenCalledOnce();
    expect(codexCore.execute).toHaveBeenCalledOnce();
    expect(claudeCore.execute.mock.calls[0]?.[0]).toEqual(
      codexCore.execute.mock.calls[0]?.[0],
    );
  });

  it("maps common flags and quoted requirements", async () => {
    const core = new RecordingCore();
    await new ClaudeCodeAdapter(core).execute(
      '/sdd.new "implement order cancellation" --change add-cancel --non-interactive --timeout 30 --json --verbose',
      "/repo",
    );
    expect(core.execute).toHaveBeenCalledWith({
      command: "new",
      cwd: "/repo",
      args: {
        requirement: "implement order cancellation",
        changeId: "add-cancel",
        nonInteractive: true,
        timeout: 30,
        json: true,
        verbose: true,
      },
    });
  });

  it("在 verbose 模式下保留 Core 返回的调试信息", async () => {
    class VerboseCore implements SddCore {
      readonly execute = vi.fn(
        async (): Promise<CommandResult> => ({
          ok: true,
          state: "INDEX_READY",
          exitCode: 0,
          data: {
            debug: {
              command: "status",
              cwd: "/repo",
              verbose: true,
            },
          },
        }),
      );
    }
    const adapter = new CodexAdapter(new VerboseCore());

    const result = await adapter.execute("sdd status --verbose", "/repo");

    expect(result).toMatchObject({
      ok: true,
      data: {
        debug: {
          command: "status",
          cwd: "/repo",
          verbose: true,
        },
      },
    });
  });

  it("默认返回人类可读摘要", async () => {
    const adapter = new CodexAdapter(new RecordingCore());

    const result = await adapter.execute("sdd status", "/repo");

    expect(result.rendered).toMatchObject({
      format: "text",
    });
    expect(result.rendered?.content).toContain("SDD Status: INDEX_READY");
  });

  it("传入 --json 时返回稳定 JSON 字符串", async () => {
    const adapter = new CodexAdapter(new RecordingCore());

    const result = await adapter.execute("sdd status --json", "/repo");

    expect(result.rendered).toMatchObject({
      format: "json",
    });
    expect(JSON.parse(result.rendered?.content ?? "")).toMatchObject({
      ok: true,
      state: "INDEX_READY",
      exitCode: 0,
    });
  });

  it.each([
    "init",
    "auto",
    "new",
    "design",
    "plan",
    "build",
    "verify",
    "review",
    "archive",
    "status",
  ] as const)(
    "returns help for %s without dispatching a write command",
    async (command) => {
      const claudeCore = new RecordingCore();
      const codexCore = new RecordingCore();
      const claude = new ClaudeCodeAdapter(claudeCore);
      const codex = new CodexAdapter(codexCore);

      expect(
        await claude.execute(`/sdd.${command} --help`, "/repo"),
      ).toMatchObject({
        ok: true,
        data: {
          command,
          description: expect.any(String),
          usage: `/sdd.${command} [options]`,
          options: expect.arrayContaining([
            expect.objectContaining({
              name: "--json",
              default: "false",
            }),
            expect.objectContaining({
              name: "--timeout <seconds>",
              default: "0",
            }),
          ]),
          examples: expect.any(Array),
          exitCodes: expect.any(Array),
        },
      });
      expect(
        await codex.execute(`sdd ${command} --help`, "/repo"),
      ).toMatchObject({
        ok: true,
        data: {
          command,
          description: expect.any(String),
          usage: `sdd ${command} [options]`,
          options: expect.arrayContaining([
            expect.objectContaining({
              name: "--json",
              default: "false",
            }),
            expect.objectContaining({
              name: "--timeout <seconds>",
              default: "0",
            }),
          ]),
          examples: expect.any(Array),
          exitCodes: expect.any(Array),
        },
      });
      const claudeHelp = await claude.execute(
        `/sdd.${command} --help`,
        "/repo",
      );
      const codexHelp = await codex.execute(`sdd ${command} --help`, "/repo");
      expect(claudeHelp.data).toMatchObject({
        examples: expect.arrayContaining([expect.stringContaining("/sdd.")]),
      });
      expect(codexHelp.data).toMatchObject({
        examples: expect.arrayContaining([expect.stringContaining("sdd ")]),
      });
      expect(claudeCore.execute).not.toHaveBeenCalled();
      expect(codexCore.execute).not.toHaveBeenCalled();
    },
  );

  it("两个宿主都返回一致的插件版本元数据", () => {
    const claude = new ClaudeCodeAdapter(new RecordingCore());
    const codex = new CodexAdapter(new RecordingCore());

    expect(claude.version()).toMatchObject({
      name: "sdd-harness",
      version: "0.1.0",
      delivery: "plugin",
      supportedTargets: ["claude-code", "codex"],
    });
    expect(codex.version()).toEqual(claude.version());
  });

  it("可通过命令直接返回版本信息且不分发到 Core", async () => {
    const claudeCore = new RecordingCore();
    const codexCore = new RecordingCore();
    const claude = new ClaudeCodeAdapter(claudeCore);
    const codex = new CodexAdapter(codexCore);

    expect(await claude.execute("/sdd.version", "/repo")).toMatchObject({
      ok: true,
      data: {
        name: "sdd-harness",
        version: "0.1.0",
        delivery: "plugin",
        supportedTargets: ["claude-code", "codex"],
      },
    });
    expect(await codex.execute("sdd --version", "/repo")).toMatchObject({
      ok: true,
      data: {
        name: "sdd-harness",
        version: "0.1.0",
        delivery: "plugin",
        supportedTargets: ["claude-code", "codex"],
      },
    });
    expect(claudeCore.execute).not.toHaveBeenCalled();
    expect(codexCore.execute).not.toHaveBeenCalled();
  });

  it("支持通过宿主运行时依赖创建 CodexAdapter", async () => {
    const executor: TaskExecutor = {
      execute: vi.fn(async ({ task }) => ({
        modifiedFiles: ["src/order.ts"],
        tddEvidence: [
          task.phase === "RED"
            ? {
                phase: task.phase,
                command: "npm test",
                passed: false,
                expectedFailure: true,
                output: "failed",
              }
            : {
                phase: task.phase,
                command: "npm test",
                passed: true,
                output: "ok",
              },
        ],
        verification:
          task.phase === "VERIFY"
            ? [{ command: "npm test", passed: true, output: "ok" }]
            : [],
      })),
    };
    const transport: McpTransport = {
      isAvailable: vi.fn().mockResolvedValue(true),
      index: vi.fn().mockResolvedValue(undefined),
      summarize: vi.fn().mockResolvedValue({
        codebaseSummary:
          "package.json\nsrc/order.ts\ntest/order.test.ts\nMCP summary",
        packageStructure: "src",
        architecture: "arch",
      }),
    };

    const root = await mkdtemp(join(tmpdir(), "sdd-codex-runtime-"));
    await writeFile(join(root, "README.md"), "# Orders\n", "utf8");
    execFileSync("git", ["init", "-b", "main"], { cwd: root });
    execFileSync("git", ["config", "user.email", "test@example.com"], {
      cwd: root,
    });
    execFileSync("git", ["config", "user.name", "SDD Harness Test"], {
      cwd: root,
    });
    execFileSync("git", ["add", "README.md"], { cwd: root });
    execFileSync("git", ["commit", "-m", "init"], { cwd: root });
    const adapter = new CodexAdapter({
      taskExecutor: executor,
      mcpTransport: transport,
    });

    expect(await adapter.execute("sdd init", root)).toMatchObject({
      ok: true,
      state: "INDEX_READY",
    });
    expect(
      JSON.parse(
        await readFile(
          join(root, ".sdd/adapters/codebase-memory-mcp/version.json"),
          "utf8",
        ),
      ),
    ).toMatchObject({
      status: "available",
    });
  });

  it("支持通过宿主运行时依赖创建 ClaudeCodeAdapter", async () => {
    const executor: TaskExecutor = {
      execute: vi.fn(async ({ task }) => ({
        modifiedFiles: ["src/order.ts"],
        tddEvidence: [
          task.phase === "RED"
            ? {
                phase: task.phase,
                command: "npm test",
                passed: false,
                expectedFailure: true,
                output: "failed",
              }
            : {
                phase: task.phase,
                command: "npm test",
                passed: true,
                output: "ok",
              },
        ],
        verification:
          task.phase === "VERIFY"
            ? [{ command: "npm test", passed: true, output: "ok" }]
            : [],
      })),
    };
    const transport: McpTransport = {
      isAvailable: vi.fn().mockResolvedValue(true),
      index: vi.fn().mockResolvedValue(undefined),
      summarize: vi.fn().mockResolvedValue({
        codebaseSummary:
          "package.json\nsrc/order.ts\ntest/order.test.ts\nMCP summary",
        packageStructure: "src",
        architecture: "arch",
      }),
    };

    const root = await mkdtemp(join(tmpdir(), "sdd-claude-runtime-"));
    await writeFile(join(root, "README.md"), "# Orders\n", "utf8");
    execFileSync("git", ["init", "-b", "main"], { cwd: root });
    execFileSync("git", ["config", "user.email", "test@example.com"], {
      cwd: root,
    });
    execFileSync("git", ["config", "user.name", "SDD Harness Test"], {
      cwd: root,
    });
    execFileSync("git", ["add", "README.md"], { cwd: root });
    execFileSync("git", ["commit", "-m", "init"], { cwd: root });
    const adapter = new ClaudeCodeAdapter({
      taskExecutor: executor,
      mcpTransport: transport,
    });

    await adapter.execute("/sdd.init", root);
    await adapter.execute(
      '/sdd.new "Implement authenticated order cancellation through an API endpoint with authorization, errors, logging, and automated tests." --change add-cancel',
      root,
    );
    await adapter.execute("/sdd.design", root);
    await adapter.execute("/sdd.plan", root);
    expect(await adapter.execute("/sdd.build", root)).toMatchObject({
      ok: true,
      state: "BUILD_READY",
      next: "sdd verify",
    });
    expect(executor.execute).toHaveBeenCalled();
  });
});
