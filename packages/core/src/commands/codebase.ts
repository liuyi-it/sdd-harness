import type { CodebaseAdapter } from "../codebase/codebase-adapter.js";
import type { CommandResult } from "../contracts.js";

/** codebase 子命令分发：status / doctor / index / query / rebuild */
export async function runCodebaseCommand(
  root: string,
  codebase: CodebaseAdapter,
  args: Record<string, unknown> | undefined,
): Promise<CommandResult> {
  const subcommand =
    (args?.subcommand as string) ??
    (args?.codebaseSubcommand as string) ??
    "status";

  const handleResult = (initResult: { provider: string; degraded: boolean }) => {
    const state = initResult.degraded
      ? { ok: true, state: "INDEX_READY" as const, exitCode: 0 }
      : { ok: true, state: "INDEX_READY" as const, exitCode: 0 };
    return state;
  };

  switch (subcommand) {
    case "status": {
      const initResult = await codebase.initialize(root);
      const capabilities = codebase.capabilities
        ? await codebase.capabilities()
        : [];
      return {
        ...handleResult(initResult),
        data: {
          provider: initResult.provider,
          degraded: initResult.degraded,
          diagnostics: initResult.diagnostics,
          capabilities,
        },
      };
    }
    case "doctor": {
      const initResult = await codebase.initialize(root);
      const d = initResult.diagnostics;
      const checks = [
        { name: "MCP installed", pass: d.installed },
        { name: "MCP configured", pass: d.configured },
        { name: "MCP connected", pass: d.connected },
        { name: "MCP callable", pass: d.callable },
        { name: "MCP indexed", pass: d.indexed },
      ];
      const failed = checks.filter((c) => !c.pass);
      const result: CommandResult = {
        ok: failed.length === 0,
        state: "INDEX_READY",
        exitCode: 0,
        data: {
          checks,
          failedCount: failed.length,
          provider: initResult.provider,
        },
      };
      if (failed.length > 0) {
        result.warnings = [
          {
            code: "W_CODEBASE_MEMORY_UNAVAILABLE",
            message: `${failed.length} 项检查未通过: ${failed.map((f) => f.name).join(", ")}`,
            next: "sdd codebase doctor",
          },
        ];
      }
      return result;
    }
    case "index": {
      const initResult = await codebase.initialize(root);
      return {
        ok: true,
        state: "INDEX_READY",
        exitCode: 0,
        data: {
          provider: initResult.provider,
          degraded: initResult.degraded,
          diagnostics: initResult.diagnostics,
        },
      };
    }
    case "rebuild": {
      const initResult = await codebase.initialize(root);
      return {
        ok: true,
        state: "INDEX_READY",
        exitCode: 0,
        data: {
          provider: initResult.provider,
          degraded: initResult.degraded,
          diagnostics: initResult.diagnostics,
        },
      };
    }
    case "query": {
      const query = (args?.query as string) || "";
      const intent = (args?.intent as string) || "impact";
      const result = await codebase.query({
        intent,
        query,
      } as Parameters<typeof codebase.query>[0]);
      return {
        ok: true,
        state: "INDEX_READY",
        exitCode: 0,
        data: result,
      };
    }
    default:
      return {
        ok: false,
        state: "FAILED",
        exitCode: 2,
        error: {
          code: "E_INVALID_PHASE_COMMAND",
          message: `codebase 子命令 ${subcommand} 暂未实现`,
          next: "sdd codebase status",
        },
      };
  }
}
