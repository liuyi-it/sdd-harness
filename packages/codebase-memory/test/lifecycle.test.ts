import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";

import {
  listAllTools,
  managedSpawnSpec,
  resolveInstalledMcp,
} from "../src/lifecycle.js";
import type { McpSession } from "../src/types.js";
import { CodebaseMemoryTransport } from "../src/transport.js";
import type { CodebaseMemoryManager } from "../src/manager.js";
import {
  collectPaginatedToolResults,
  decodeToolResult,
} from "../src/manager.js";

describe("MCP lifecycle", () => {
  it("进程启动配置不包含生命周期 timeout，并兼容 Windows cmd", () => {
    const posix = managedSpawnSpec("0.9.0", "darwin");
    expect(posix).toMatchObject({ command: "npx" });
    expect(posix.options).not.toHaveProperty("timeout");

    const windows = managedSpawnSpec("0.9.0", "win32", "C:\\cmd.exe");
    expect(windows.command).toBe("C:\\cmd.exe");
    expect(windows.args).toEqual([
      "/d",
      "/s",
      "/c",
      "npx",
      "-y",
      "codebase-memory-mcp@0.9.0",
    ]);
    expect(windows.options).not.toHaveProperty("timeout");
  });

  it("优先解析项目本地安装，再解析 npm 全局安装", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-mcp-resolution-"));
    const globalRoot = join(root, "global-node-modules");
    await writeFakeMcp(join(globalRoot, "codebase-memory-mcp"), "0.9.0");

    const global = await resolveInstalledMcp(root, "0.9.0", { globalRoot });
    expect(global).toMatchObject({ source: "global", version: "0.9.0" });

    await writeFakeMcp(
      join(root, "node_modules", "codebase-memory-mcp"),
      "0.9.0",
    );
    const local = await resolveInstalledMcp(root, "0.9.0", { globalRoot });
    expect(local).toMatchObject({ source: "local", version: "0.9.0" });
    expect(local?.spawnSpec.command).toBe(process.execPath);
  });

  it("忽略与锁定版本不一致的本地和全局安装", async () => {
    const root = await mkdtemp(join(tmpdir(), "sdd-mcp-version-"));
    const globalRoot = join(root, "global-node-modules");
    await writeFakeMcp(
      join(root, "node_modules", "codebase-memory-mcp"),
      "1.0.0",
    );
    await writeFakeMcp(join(globalRoot, "codebase-memory-mcp"), "1.0.0");

    await expect(
      resolveInstalledMcp(root, "0.9.0", { globalRoot }),
    ).resolves.toBeUndefined();
  });

  it("遍历 tools/list 全部分页并保留后续页工具", async () => {
    const call = vi
      .fn()
      .mockResolvedValueOnce({
        tools: [{ name: "index_repository" }, { name: "search_graph" }],
        nextCursor: "page-2",
      })
      .mockResolvedValueOnce({
        tools: [{ name: "detect_changes" }, { name: "get_architecture" }],
      });
    const session: McpSession = {
      call,
      notify: vi.fn(),
      close: vi.fn(),
      isAlive: () => true,
      onExit: vi.fn(),
    };

    await expect(listAllTools(session, 1_000)).resolves.toEqual([
      "index_repository",
      "search_graph",
      "detect_changes",
      "get_architecture",
    ]);
    expect(call).toHaveBeenNthCalledWith(1, "tools/list", {});
    expect(call).toHaveBeenNthCalledWith(2, "tools/list", {
      cursor: "page-2",
    });
  });
});

async function writeFakeMcp(
  packageRoot: string,
  version: string,
): Promise<void> {
  await mkdir(join(packageRoot, "dist"), { recursive: true });
  await writeFile(
    join(packageRoot, "package.json"),
    JSON.stringify({
      name: "codebase-memory-mcp",
      version,
      bin: { "codebase-memory-mcp": "dist/index.js" },
    }),
  );
  await writeFile(join(packageRoot, "dist", "index.js"), "");
}

describe("MCP transport contract", () => {
  it("根据 has_more 自动请求后续查询页", async () => {
    const callPage = vi
      .fn()
      .mockResolvedValueOnce({
        structuredContent: { results: [{ name: "A" }], has_more: true },
      })
      .mockResolvedValueOnce({
        structuredContent: { results: [{ name: "B" }], has_more: false },
      });

    await expect(
      collectPaginatedToolResults(callPage, { project: "repo", offset: 0 }),
    ).resolves.toHaveLength(2);
    expect(callPage).toHaveBeenNthCalledWith(2, {
      project: "repo",
      offset: 1,
    });
  });

  it("按 intent 解码 structuredContent 中的 test、route 与 symbol", () => {
    expect(
      decodeToolResult("tests", {
        structuredContent: {
          results: [
            {
              file_path: "test/order.test.ts",
              qualified_name: "OrderTest",
              label: "Function",
            },
          ],
        },
      }),
    ).toEqual([
      expect.objectContaining({
        type: "test",
        path: "test/order.test.ts",
        symbol: "OrderTest",
      }),
    ]);
    expect(
      decodeToolResult("routes", {
        content: [
          {
            text: JSON.stringify({
              results: [{ path: "/orders/:id", name: "cancelOrder" }],
            }),
          },
        ],
      }),
    ).toEqual([
      expect.objectContaining({ type: "route", symbol: "cancelOrder" }),
    ]);
  });

  it("将合法的空 MCP 查询结果保持为精确空结果", () => {
    expect(
      decodeToolResult("symbols", {
        structuredContent: { total: 0, results: [], has_more: false },
      }),
    ).toEqual([]);
  });

  it("完整转发 CLI query 文本，并区分 tools 与 intents", async () => {
    const query = vi.fn().mockResolvedValue({
      provider: "fallback-file-scan",
      intent: "impact",
      degraded: true,
      items: [],
    });
    const manager = {
      query,
      getCapabilities: vi.fn().mockResolvedValue({
        availableTools: ["detect_changes", "search_graph"],
        supportedIntents: ["impact", "symbols"],
      }),
    } as unknown as CodebaseMemoryManager;
    const transport = new CodebaseMemoryTransport(manager);

    await transport.query("/repo", { intent: "impact", query: "OrderService" });
    await expect(transport.capabilities("/repo")).resolves.toEqual({
      availableTools: ["detect_changes", "search_graph"],
      supportedIntents: ["impact", "symbols"],
    });
    expect(query).toHaveBeenCalledWith({
      intent: "impact",
      query: "OrderService",
      root: "/repo",
    });
  });

  it("降级查询保留 fallback 发现的文件、符号、测试与风险", async () => {
    const manager = {
      query: vi.fn().mockResolvedValue({
        provider: "fallback-file-scan",
        intent: "impact",
        degraded: true,
        items: [
          { type: "file", path: "src/order.ts", confidence: 1, reason: "匹配" },
          {
            type: "symbol",
            symbol: "cancelOrder",
            confidence: 1,
            reason: "匹配",
          },
          {
            type: "test",
            path: "test/order.test.ts",
            confidence: 1,
            reason: "匹配",
          },
          { type: "risk", path: "src/order.ts", confidence: 1, reason: "风险" },
        ],
      }),
      getCapabilities: vi.fn(),
    } as unknown as CodebaseMemoryManager;
    const result = await new CodebaseMemoryTransport(manager).query("/repo", {
      intent: "impact",
      query: "OrderService",
    });

    expect(result).toMatchObject({
      provider: "fallback-file-scan",
      degraded: true,
      payload: {
        files: ["src/order.ts"],
        symbols: ["cancelOrder"],
        tests: ["test/order.test.ts"],
        risks: ["src/order.ts"],
      },
    });
  });
});
