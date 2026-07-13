import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  CodebaseAdapter,
  MCP_UNAVAILABLE_REASON,
  type McpTransport,
} from "../src/codebase/codebase-adapter.js";
import { PINNED_DEPENDENCIES } from "../src/dependencies.js";

// 这里既验证固定依赖元数据，也验证 MCP 可用/不可用两条代码库上下文路径。
const roots: string[] = [];

async function project(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "sdd-codebase-"));
  roots.push(root);
  await mkdir(join(root, "src/services"), { recursive: true });
  await writeFile(
    join(root, "src/services/order.ts"),
    "export class OrderService {}\n",
  );
  await writeFile(
    join(root, "CLAUDE.md.meta.json"),
    '{"createdAt":"2026-07-04T00:00:00.000Z"}\n',
  );
  await writeFile(join(root, "src/application-prod.yml"), "password: secret\n");
  await writeFile(join(root, "src/server.pem"), "private key\n");
  await mkdir(join(root, "node_modules/ignored"), { recursive: true });
  await writeFile(join(root, "node_modules/ignored/index.ts"), "ignored\n");
  return root;
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("pinned dependencies", () => {
  it("uses the exact approved upstream versions and commits", () => {
    expect(PINNED_DEPENDENCIES).toMatchObject({
      codebaseMemoryMcp: {
        version: "v0.9.0",
        commit: "b637e3330c96cfe452da623db068c241aaa3ec01",
      },
      openSpec: {
        version: "v1.4.1",
        commit: "1b06fddd59d8e592d5b5794a1970b22867e85b1f",
      },
      superpowers: {
        version: "v6.1.1",
        commit: "d884ae04edebef577e82ff7c4e143debd0bbec99",
      },
      mattpocockSkills: {
        version: "main@391a270",
        commit: "391a2701dd948f94f56a39f7533f8eea9a859c87",
      },
    });
  });
});

describe("CodebaseAdapter", () => {
  it("indexes and reads summaries through an available MCP transport", async () => {
    const root = await project();
    const transport: McpTransport = {
      inspect: vi.fn().mockResolvedValue({
        installed: true,
        configured: true,
      }),
      isAvailable: vi.fn().mockResolvedValue(true),
      index: vi.fn().mockResolvedValue(undefined),
      summarize: vi.fn().mockResolvedValue({
        codebaseSummary: "MCP summary",
        packageStructure: "src/services",
        architecture: "service module",
      }),
    };

    const result = await new CodebaseAdapter(transport).initialize(root);

    expect(transport.index).toHaveBeenCalledWith(root);
    expect(result).toMatchObject({
      provider: "codebase-memory-mcp",
      degraded: false,
      codebaseSummary: "MCP summary",
      diagnostics: {
        installed: true,
        configured: true,
        connected: true,
        callable: true,
        indexed: true,
      },
    });
  });

  it("falls back to a bounded repository scan when MCP is unavailable", async () => {
    const root = await project();
    const transport: McpTransport = {
      inspect: vi.fn().mockResolvedValue({
        installed: true,
        configured: true,
        connected: false,
        callable: false,
        indexed: false,
      }),
      isAvailable: vi.fn().mockResolvedValue(false),
      index: vi.fn(),
      summarize: vi.fn(),
    };

    const result = await new CodebaseAdapter(transport).initialize(root);

    expect(result.provider).toBe("fallback-file-scan");
    expect(result.degraded).toBe(true);
    expect(result.reason).toBe(MCP_UNAVAILABLE_REASON);
    expect(result.diagnostics).toMatchObject({
      installed: true,
      configured: true,
      connected: false,
      callable: false,
      indexed: false,
    });
    expect(result.codebaseSummary).toContain("src/services/order.ts");
    expect(result.codebaseSummary).toContain("## 关键字扫描");
    expect(result.codebaseSummary).toContain("## 候选文件摘要");
    expect(result.codebaseSummary).not.toContain("node_modules");
    expect(result.codebaseSummary).not.toContain("application-prod.yml");
    expect(result.codebaseSummary).not.toContain("server.pem");
    expect(result.codebaseSummary).not.toContain("CLAUDE.md.meta.json");
  });

  it("在 MCP 可见但调用失败时自动降级而不是直接失败", async () => {
    const root = await project();
    const transport: McpTransport = {
      inspect: vi.fn().mockResolvedValue({
        installed: true,
        configured: true,
        connected: true,
      }),
      isAvailable: vi.fn().mockResolvedValue(true),
      index: vi.fn().mockRejectedValue(new Error("MCP 调用失败")),
      summarize: vi.fn(),
    };

    const result = await new CodebaseAdapter(transport).initialize(root);

    expect(result).toMatchObject({
      provider: "fallback-file-scan",
      degraded: true,
      reason: MCP_UNAVAILABLE_REASON,
      diagnostics: {
        installed: true,
        configured: true,
        connected: true,
        callable: false,
        indexed: false,
        message: "MCP 调用失败",
      },
    });
  });

  it("使用初始化后的 diagnostics，而不是启动前的 false 快照", async () => {
    const root = await project();
    const inspect = vi
      .fn()
      .mockResolvedValueOnce({
        installed: false,
        configured: false,
        connected: false,
        callable: false,
        indexed: false,
      })
      .mockResolvedValueOnce({
        installed: true,
        configured: true,
        connected: true,
        callable: true,
        indexed: true,
      });
    const result = await new CodebaseAdapter({
      inspect,
      isAvailable: vi.fn().mockResolvedValue(true),
      index: vi.fn().mockResolvedValue(undefined),
      summarize: vi.fn().mockResolvedValue({
        codebaseSummary: "MCP summary",
        packageStructure: "src",
        architecture: "test",
      }),
    }).initialize(root);
    expect(inspect).toHaveBeenCalledTimes(2);
    expect(result.diagnostics).toMatchObject({
      connected: true,
      callable: true,
      indexed: true,
    });
  });

  it("MCP 必需但不可用时不伪装成成功初始化", async () => {
    const root = await project();
    await expect(
      new CodebaseAdapter({
        isAvailable: vi.fn().mockResolvedValue(true),
        index: vi.fn().mockResolvedValue({
          degraded: false,
          failed: true,
          reason: "MCP handshake failed",
        }),
        summarize: vi.fn(),
      }).initialize(root),
    ).rejects.toMatchObject({ code: "E_COMPONENT_UNAVAILABLE" });
  });
});
