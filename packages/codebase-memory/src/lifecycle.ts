import { execFile, spawn, type ChildProcess } from "node:child_process";
import { access, readFile } from "node:fs/promises";
import { basename, isAbsolute, join, resolve, win32 } from "node:path";
import type { McpLifecycleResult, McpSession } from "./types.js";

export type McpProgressReporter = (message: string) => void;

export interface McpSpawnSpec {
  command: string;
  args: string[];
  options: { stdio: ["pipe", "pipe", "pipe"] };
}

export interface InstalledMcp {
  source: "local" | "global" | "standalone";
  version: string;
  packageRoot: string;
  spawnSpec: McpSpawnSpec;
}

export interface ResolveInstalledMcpOptions {
  globalRoot?: string;
  globalRootResolver?: (
    platform: NodeJS.Platform,
  ) => Promise<string | undefined>;
  executablePath?: string;
  platform?: NodeJS.Platform;
  env?: NodeJS.ProcessEnv;
}

export interface StartManagedMcpOptions {
  onProgress?: McpProgressReporter;
  globalRoot?: string;
}

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

/** 优先解析项目本地和 npm 全局安装，再查找独立二进制。 */
export async function resolveInstalledMcp(
  root: string,
  expectedVersion: string,
  options: ResolveInstalledMcpOptions = {},
): Promise<InstalledMcp | undefined> {
  const platform = options.platform ?? process.platform;
  const local = await installedMcpAt(
    join(root, "node_modules", "codebase-memory-mcp"),
    "local",
    expectedVersion,
    platform,
  );
  if (local !== undefined) return local;

  const globalRoot =
    options.globalRoot ??
    (await resolveGlobalRootSafely(
      options.globalRootResolver ?? npmGlobalRoot,
      platform,
    ));
  if (globalRoot !== undefined) {
    const global = await installedMcpAt(
      join(globalRoot, "codebase-memory-mcp"),
      "global",
      expectedVersion,
      platform,
    );
    if (global !== undefined) return global;
  }

  return resolveStandaloneMcp(expectedVersion, {
    ...options,
    platform,
  });
}

async function installedMcpAt(
  packageRoot: string,
  source: "local" | "global",
  expectedVersion: string,
  platform: NodeJS.Platform,
): Promise<InstalledMcp | undefined> {
  try {
    const packageJson = JSON.parse(
      await readFile(join(packageRoot, "package.json"), "utf8"),
    ) as { name?: unknown; version?: unknown; bin?: unknown };
    if (
      packageJson.name !== "codebase-memory-mcp" ||
      packageJson.version !== expectedVersion
    )
      return undefined;
    const bin = packageBin(packageJson.bin);
    if (bin === undefined) return undefined;
    const entry = isAbsolute(bin) ? bin : resolve(packageRoot, bin);
    const packagedExecutable = join(
      packageRoot,
      "bin",
      platform === "win32" ? "codebase-memory-mcp.exe" : "codebase-memory-mcp",
    );
    if (await pathExists(packagedExecutable)) {
      return {
        source,
        version: expectedVersion,
        packageRoot,
        spawnSpec: directSpawnSpec(packagedExecutable),
      };
    }
    // 官方 npm 包的 bin.js 只是下载壳。真实二进制缺失时不能把它当作
    // 可用安装，否则 Windows 会在启动阶段无限等待 GitHub Release 下载。
    if (basename(entry).toLowerCase() === "bin.js") return undefined;
    await access(entry);
    return {
      source,
      version: expectedVersion,
      packageRoot,
      spawnSpec: {
        command: process.execPath,
        args: [entry],
        options: { stdio: ["pipe", "pipe", "pipe"] },
      },
    };
  } catch {
    return undefined;
  }
}

async function resolveStandaloneMcp(
  expectedVersion: string,
  options: ResolveInstalledMcpOptions & { platform: NodeJS.Platform },
): Promise<InstalledMcp | undefined> {
  for (const candidate of standaloneMcpCandidates(options)) {
    if (!(await pathExists(candidate))) continue;
    return {
      source: "standalone",
      version: expectedVersion,
      packageRoot: candidate,
      spawnSpec: directSpawnSpec(candidate),
    };
  }
  return undefined;
}

function standaloneMcpCandidates(
  options: ResolveInstalledMcpOptions & { platform: NodeJS.Platform },
): string[] {
  const env = options.env ?? process.env;
  const candidates = [
    options.executablePath,
    envValue(env, "CODEBASE_MEMORY_MCP_PATH"),
  ];
  if (options.platform === "win32") {
    const localAppData = envValue(env, "LOCALAPPDATA");
    if (localAppData !== undefined) {
      candidates.push(
        win32.join(
          localAppData,
          "Programs",
          "codebase-memory-mcp",
          "codebase-memory-mcp.exe",
        ),
        win32.join(
          localAppData,
          "codebase-memory-mcp",
          "codebase-memory-mcp.exe",
        ),
      );
    }
  }

  const pathValue = envValue(env, "PATH");
  if (pathValue !== undefined) {
    const separator = options.platform === "win32" ? ";" : ":";
    const executable =
      options.platform === "win32"
        ? "codebase-memory-mcp.exe"
        : "codebase-memory-mcp";
    for (const directory of pathValue.split(separator)) {
      const normalized = directory.trim().replace(/^"|"$/g, "");
      if (normalized.length === 0) continue;
      candidates.push(
        options.platform === "win32"
          ? win32.join(normalized, executable)
          : join(normalized, executable),
      );
    }
  }
  return [
    ...new Set(candidates.filter((path): path is string => path !== undefined)),
  ];
}

function envValue(env: NodeJS.ProcessEnv, name: string): string | undefined {
  const entry = Object.entries(env).find(
    ([key, value]) => key.toUpperCase() === name && value !== undefined,
  );
  return entry?.[1];
}

function directSpawnSpec(command: string): McpSpawnSpec {
  return {
    command,
    args: [],
    options: { stdio: ["pipe", "pipe", "pipe"] },
  };
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

function packageBin(bin: unknown): string | undefined {
  if (typeof bin === "string") return bin;
  if (bin === null || typeof bin !== "object") return undefined;
  const entries = bin as Record<string, unknown>;
  const preferred = entries["codebase-memory-mcp"];
  if (typeof preferred === "string") return preferred;
  return Object.values(entries).find(
    (value): value is string => typeof value === "string",
  );
}

async function npmGlobalRoot(
  platform: NodeJS.Platform,
): Promise<string | undefined> {
  const spec = npmGlobalRootSpec(platform);
  return new Promise((resolveGlobalRoot) => {
    try {
      execFile(
        spec.command,
        spec.args,
        { timeout: 10_000, encoding: "utf8", windowsHide: true },
        (error, stdout) => {
          const root = stdout.trim();
          resolveGlobalRoot(
            error === null && root.length > 0 ? root : undefined,
          );
        },
      );
    } catch {
      // Windows 无法直接启动 .cmd 等同步异常时，继续探测独立二进制。
      resolveGlobalRoot(undefined);
    }
  });
}

export function npmGlobalRootSpec(
  platform: NodeJS.Platform,
  comspec = process.env.ComSpec,
): { command: string; args: string[] } {
  return platform === "win32"
    ? {
        command: comspec ?? "cmd.exe",
        args: ["/d", "/s", "/c", "npm", "root", "--global"],
      }
    : { command: "npm", args: ["root", "--global"] };
}

async function resolveGlobalRootSafely(
  resolver: (platform: NodeJS.Platform) => Promise<string | undefined>,
  platform: NodeJS.Platform,
): Promise<string | undefined> {
  try {
    return await resolver(platform);
  } catch {
    return undefined;
  }
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
    child.on("error", (error) => this.rejectPending(error));
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
    this.rejectPending(new Error("MCP stdio 会话已关闭"));
  }

  private rejectPending(error: Error): void {
    for (const { reject } of this.pending.values()) reject(error);
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
  options: StartManagedMcpOptions = {},
): Promise<McpLifecycleResult> {
  options.onProgress?.("正在检查项目本地、npm 全局及独立二进制…");
  const installed = await resolveInstalledMcp(root, version, {
    ...(options.globalRoot === undefined
      ? {}
      : { globalRoot: options.globalRoot }),
  });
  if (installed !== undefined) {
    const sourceLabel =
      installed.source === "local"
        ? "项目本地"
        : installed.source === "global"
          ? "npm 全局"
          : "独立二进制";
    const versionLabel =
      installed.source === "standalone" ? "" : ` v${installed.version}`;
    options.onProgress?.(`发现${sourceLabel}${versionLabel}，正在启动…`);
    const direct = await startMcpProcess(
      root,
      installed.spawnSpec,
      timeoutMs,
      options.onProgress,
    );
    if (direct.status === "STARTED") return direct;
    options.onProgress?.(`已安装版本启动失败，改用 npx：${direct.message}`);
  } else {
    options.onProgress?.(
      `未找到匹配的已安装版本 v${version}，将通过 npx 获取并启动…`,
    );
  }
  return startMcpProcess(
    root,
    managedSpawnSpec(version),
    timeoutMs,
    options.onProgress,
  );
}

async function startMcpProcess(
  root: string,
  spec: McpSpawnSpec,
  timeoutMs: number,
  onProgress?: McpProgressReporter,
): Promise<McpLifecycleResult> {
  const child: ChildProcess = spawn(spec.command, spec.args, {
    ...spec.options,
    cwd: root,
  });
  let stderr = "";
  child.stderr?.setEncoding("utf8");
  child.stderr?.on("data", (chunk: string) => {
    stderr = `${stderr}${chunk}`.slice(-8_192);
  });
  const session = new StdioMcpSession(child);
  const startedAt = Date.now();
  const progressTimer = setInterval(() => {
    const seconds = Math.max(1, Math.round((Date.now() - startedAt) / 1_000));
    onProgress?.(`MCP 仍在启动，已等待 ${seconds} 秒…`);
  }, 5_000);
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
    onProgress?.("MCP 已连接，正在读取工具清单…");
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
    onProgress?.(`MCP 启动完成，已发现 ${tools.length} 个工具`);
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
      message: failureMessage(error, stderr),
    };
  } finally {
    clearInterval(progressTimer);
  }
}

function failureMessage(error: unknown, stderr: string): string {
  const reason = error instanceof Error ? error.message : String(error);
  const detail = stderr
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(-3)
    .join(" | ");
  return detail.length === 0 ? reason : `${reason}；进程输出：${detail}`;
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
