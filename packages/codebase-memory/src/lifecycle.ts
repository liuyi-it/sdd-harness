import { spawn, type ChildProcess } from "node:child_process";
import type { McpLifecycleResult, McpSession } from "./types.js";

class StdioMcpSession implements McpSession {
  private nextId = 1;
  private buffer = "";
  private readonly pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (reason: Error) => void }
  >();

  constructor(private readonly child: ChildProcess) {
    child.stdout?.setEncoding("utf8");
    child.stdout?.on("data", (chunk: string) => this.onData(chunk));
    child.on("exit", () => this.close());
  }

  async call(
    method: string,
    params: Record<string, unknown> = {},
  ): Promise<unknown> {
    const id = this.nextId++;
    const response = new Promise<unknown>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });
    this.send({ jsonrpc: "2.0", id, method, params });
    return response;
  }

  notify(method: string, params: Record<string, unknown> = {}): void {
    this.send({ jsonrpc: "2.0", method, params });
  }

  close(): void {
    for (const { reject } of this.pending.values())
      reject(new Error("MCP stdio 会话已关闭"));
    this.pending.clear();
  }

  private send(message: Record<string, unknown>): void {
    if (this.child.stdin === null || this.child.killed)
      throw new Error("MCP stdio 不可写");
    this.child.stdin.write(`${JSON.stringify(message)}\n`);
  }

  private onData(chunk: string): void {
    this.buffer += chunk;
    let newline = this.buffer.indexOf("\n");
    while (newline >= 0) {
      const line = this.buffer.slice(0, newline).trim();
      this.buffer = this.buffer.slice(newline + 1);
      newline = this.buffer.indexOf("\n");
      if (line.length === 0) continue;
      try {
        const message = JSON.parse(line) as {
          id?: number;
          result?: unknown;
          error?: { message?: string };
        };
        if (typeof message.id !== "number") continue;
        const pending = this.pending.get(message.id);
        if (pending === undefined) continue;
        this.pending.delete(message.id);
        if (message.error !== undefined)
          pending.reject(new Error(message.error.message ?? "MCP 请求失败"));
        else pending.resolve(message.result);
      } catch {
        // MCP stderr/stdout 可能包含诊断文本；只忽略无法解析的单行。
      }
    }
  }
}

/**
 * 通过 npx 启动 codebase-memory-mcp managed runtime
 * 使用 stdio 通信，完成基本的 MCP 进程生命周期管理
 */
export async function startManagedMcp(
  root: string,
  version: string,
  timeoutMs: number,
): Promise<McpLifecycleResult> {
  const child: ChildProcess = spawn(
    "npx",
    ["-y", `codebase-memory-mcp@${version}`],
    {
      cwd: root,
      stdio: ["pipe", "pipe", "pipe"],
      timeout: timeoutMs,
    },
  );
  // 消费 stderr，避免 MCP 的诊断输出填满 pipe 后阻塞协议进程。
  child.stderr?.resume();
  const session = new StdioMcpSession(child);
  try {
    const withTimeout = <T>(promise: Promise<T>) =>
      Promise.race<T>([
        promise,
        new Promise<T>((_, reject) =>
          setTimeout(
            () => reject(new Error(`MCP 启动超时 (${timeoutMs}ms)`)),
            timeoutMs,
          ),
        ),
      ]);
    await withTimeout(
      session.call("initialize", {
        protocolVersion: "2024-11-05",
        capabilities: {},
        clientInfo: { name: "sdd-harness", version: "0.1.0" },
      }),
    );
    session.notify("notifications/initialized");
    const listed = await withTimeout(session.call("tools/list"));
    const tools = Array.isArray((listed as { tools?: unknown }).tools)
      ? (listed as { tools: Array<{ name?: unknown }> }).tools
          .map((tool) => tool.name)
          .filter((name): name is string => typeof name === "string")
      : [];
    return {
      provider: "codebase-memory-mcp",
      mode: "managed",
      status: "STARTED",
      ...(child.pid === undefined ? {} : { pid: child.pid }),
      session,
      tools,
    };
  } catch (error) {
    session.close();
    child.kill();
    return {
      provider: "codebase-memory-mcp",
      mode: "managed",
      status: "UNAVAILABLE",
      ...(child.pid === undefined ? {} : { pid: child.pid }),
      message: error instanceof Error ? error.message : String(error),
    };
  }
}

/** 停止 managed MCP 进程 */
export function stopManagedMcp(result: McpLifecycleResult): void {
  if (result.pid) {
    try {
      process.kill(result.pid, "SIGTERM");
      // 等待 2 秒后强制 SIGKILL
      setTimeout(() => {
        try {
          process.kill(result.pid!, "SIGKILL");
        } catch {
          // 进程已退出
        }
      }, 2000);
    } catch {
      // 进程可能已退出
    }
  }
}
