import { spawn, type ChildProcess } from "node:child_process";
import type { McpLifecycleResult } from "./types.js";

/**
 * 通过 npx 启动 codebase-memory-mcp managed runtime
 * 使用 stdio 通信，完成基本的 MCP 进程生命周期管理
 */
export function startManagedMcp(
  root: string,
  version: string,
  timeoutMs: number,
): Promise<McpLifecycleResult> {
  return new Promise((resolve) => {
    let settled = false;

    const finish = (status: McpLifecycleResult["status"], message?: string) => {
      if (settled) return;
      settled = true;
      const result: McpLifecycleResult = {
        provider: "codebase-memory-mcp",
        mode: "managed",
        status,
      };
      if (message !== undefined) result.message = message;
      if (child.pid !== undefined) result.pid = child.pid;
      resolve(result);
    };

    const child: ChildProcess = spawn(
      "npx",
      ["-y", `codebase-memory-mcp@${version}`],
      {
        cwd: root,
        stdio: ["pipe", "pipe", "pipe"],
        timeout: timeoutMs,
      },
    );

    const timeout = setTimeout(() => {
      if (!settled) {
        child.kill();
        finish("UNAVAILABLE", `MCP 启动超时 (${timeoutMs}ms)`);
      }
    }, timeoutMs);

    child.on("error", (err) => {
      clearTimeout(timeout);
      finish("FAILED", err.message);
    });

    child.on("exit", (code) => {
      clearTimeout(timeout);
      if (!settled) {
        finish("FAILED", `MCP 进程意外退出，退出码: ${code ?? "null"}`);
      }
    });

    // 简单握手检测：进程启动 1 秒后未崩溃视为 STARTED
    setTimeout(() => {
      if (!settled) {
        clearTimeout(timeout);
        finish("STARTED");
      }
    }, 1000);
  });
}

/** 停止 managed MCP 进程 */
export function stopManagedMcp(result: McpLifecycleResult): void {
  if (result.pid) {
    try {
      process.kill(result.pid, "SIGTERM");
    } catch {
      // 进程可能已退出
    }
  }
}
