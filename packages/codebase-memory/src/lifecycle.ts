import { spawn, type ChildProcess } from "node:child_process";
import type { McpLifecycleResult, McpSession } from "./types.js";

export function managedSpawnSpec(
  version: string,
  platform: NodeJS.Platform = process.platform,
  comspec = process.env.ComSpec,
): {
  command: string;
  args: string[];
  options: { stdio: ["pipe", "pipe", "pipe"] };
} {
  if (!/^\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?$/.test(version))
    throw new Error(`非法 MCP 版本：${version}`);
  return platform === "win32"
    ? {
        command: comspec ?? "cmd.exe",
        args: ["/d", "/s", "/c", "npx", "-y", `codebase-memory-mcp@${version}`],
        options: { stdio: ["pipe", "pipe", "pipe"] },
      }
    : {
        command: "npx",
        args: ["-y", `codebase-memory-mcp@${version}`],
        options: { stdio: ["pipe", "pipe", "pipe"] },
      };
}

class StdioMcpSession implements McpSession {
  private nextId = 1;
  private buffer = "";
  private readonly pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (reason: Error) => void }
  >();
  private readonly exitListeners: Array<() => void> = [];

  constructor(private readonly child: ChildProcess) {
    child.stdout?.setEncoding("utf8");
    child.stdout?.on("data", (chunk: string) => this.onData(chunk));
    child.on("exit", () => {
      this.close();
      for (const listener of this.exitListeners) listener();
    });
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

  isAlive(): boolean {
    return !this.child.killed && this.child.exitCode === null;
  }

  onExit(listener: () => void): void {
    this.exitListeners.push(listener);
  }

  private send(message: Record<string, unknown>): void {
    if (this.child.stdin === null || this.child.killed)
      throw new Error("MCP stdio 不可写");
    this.child.stdin.write(`${JSON.stringify(message)}\n`);
  }

  private onData(chunk: string): void {
    this.buffer += chunk;
    while (true) {
      const framed = this.readContentLengthFrame();
      if (framed === null) break;
      if (framed !== undefined) {
        this.handleMessage(framed);
        continue;
      }
      const newline = this.buffer.indexOf("\n");
      if (newline < 0) break;
      const line = this.buffer.slice(0, newline).trim();
      this.buffer = this.buffer.slice(newline + 1);
      if (line.length > 0) this.handleMessage(line);
    }
  }

  /** 同时兼容 JSONL 与 MCP Content-Length 分帧。 */
  private readContentLengthFrame(): string | null | undefined {
    if (!this.buffer.startsWith("Content-Length:")) return undefined;
    const headerEnd = this.buffer.indexOf("\r\n\r\n");
    const separatorLength = headerEnd >= 0 ? 4 : 2;
    const normalizedHeaderEnd =
      headerEnd >= 0 ? headerEnd : this.buffer.indexOf("\n\n");
    if (normalizedHeaderEnd < 0) return null;
    const header = this.buffer.slice(0, normalizedHeaderEnd);
    const match = /^Content-Length:\s*(\d+)\s*$/im.exec(header);
    if (match === null) {
      this.buffer = this.buffer.slice(normalizedHeaderEnd + separatorLength);
      return "";
    }
    const bodyStart = normalizedHeaderEnd + separatorLength;
    const bodyEnd = bodyStart + Number(match[1]);
    if (this.buffer.length < bodyEnd) return null;
    const body = this.buffer.slice(bodyStart, bodyEnd);
    this.buffer = this.buffer.slice(bodyEnd);
    return body;
  }

  private handleMessage(line: string): void {
    try {
      const message = JSON.parse(line) as {
        id?: number;
        result?: unknown;
        error?: { message?: string };
      };
      if (typeof message.id !== "number") return;
      const pending = this.pending.get(message.id);
      if (pending === undefined) return;
      this.pending.delete(message.id);
      if (message.error !== undefined)
        pending.reject(new Error(message.error.message ?? "MCP 请求失败"));
      else pending.resolve(message.result);
    } catch {
      // MCP stdout 可能包含诊断文本；忽略无法解析的独立消息。
    }
  }
}

export async function listAllTools(
  session: McpSession,
  timeoutMs: number,
): Promise<string[]> {
  const tools: string[] = [];
  let cursor: string | undefined;
  do {
    const listed = await callWithTimeout(
      session.call("tools/list", cursor === undefined ? {} : { cursor }),
      timeoutMs,
      "tools/list",
    );
    const page = listed as { tools?: unknown; nextCursor?: unknown };
    if (!Array.isArray(page.tools)) throw new Error("MCP tools/list 响应无效");
    tools.push(
      ...(page.tools as Array<{ name?: unknown }>)
        .map((tool) => tool.name)
        .filter((name): name is string => typeof name === "string"),
    );
    cursor =
      typeof page.nextCursor === "string" && page.nextCursor.length > 0
        ? page.nextCursor
        : undefined;
  } while (cursor !== undefined);
  return [...new Set(tools)];
}

async function callWithTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number,
  operation: string,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<T>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`MCP ${operation} 超时 (${timeoutMs}ms)`)),
          timeoutMs,
        );
      }),
    ]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
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
  const { command, args, options } = managedSpawnSpec(version);
  const child: ChildProcess = spawn(command, args, {
    ...options,
    cwd: root,
  });
  // 消费 stderr，避免 MCP 的诊断输出填满 pipe 后阻塞协议进程。
  child.stderr?.resume();
  const session = new StdioMcpSession(child);
  try {
    await callWithTimeout(
      session.call("initialize", {
        protocolVersion: "2024-11-05",
        capabilities: {},
        clientInfo: { name: "sdd-harness", version: "0.1.0" },
      }),
      timeoutMs,
      "initialize",
    );
    session.notify("notifications/initialized");
    const tools = await listAllTools(session, timeoutMs);
    const lifecycle: McpLifecycleResult = {
      provider: "codebase-memory-mcp",
      mode: "managed",
      status: "STARTED",
      ...(child.pid === undefined ? {} : { pid: child.pid }),
      session,
      tools,
    };
    session.onExit(() => {
      lifecycle.status = "FAILED";
      lifecycle.message = "MCP 进程已退出";
    });
    return lifecycle;
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
  result.session?.close();
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
