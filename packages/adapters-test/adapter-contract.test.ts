import { describe, expect, it, vi } from "vitest";

import {
  type CommandRequest,
  type CommandResult,
  type SddCore,
} from "../core/src/contracts.js";
import { ClaudeCodeAdapter } from "../claude-code-plugin/src/adapter.js";
import { CodexAdapter } from "../codex-plugin/src/adapter.js";

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

describe("adapter parity", () => {
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

  it("returns help and version without dispatching a write command", async () => {
    const core = new RecordingCore();
    const adapter = new CodexAdapter(core);
    expect(await adapter.execute("sdd status --help", "/repo")).toMatchObject({
      ok: true,
      data: { command: "status", options: expect.any(Array) },
    });
    expect(adapter.version()).toMatchObject({
      name: "sdd-harness",
      version: "0.1.0",
      delivery: "plugin",
    });
    expect(core.execute).not.toHaveBeenCalled();
  });
});
