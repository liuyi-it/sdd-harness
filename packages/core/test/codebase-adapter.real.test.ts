import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { afterEach, describe, expect, it } from "vitest";

import {
  CodebaseAdapter,
  type McpTransport,
} from "../src/codebase/codebase-adapter.js";

const roots: string[] = [];
const realTransportModule = process.env.SDD_REAL_MCP_TRANSPORT_MODULE;

async function project(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "sdd-real-mcp-"));
  roots.push(root);
  await writeFile(join(root, "README.md"), "# Real MCP fixture\n", "utf8");
  await writeFile(
    join(root, "service.ts"),
    "export function service() { return 'ok'; }\n",
    "utf8",
  );
  return root;
}

async function loadTransport(): Promise<McpTransport> {
  if (realTransportModule === undefined) {
    throw new Error("缺少 SDD_REAL_MCP_TRANSPORT_MODULE");
  }
  const loaded = (await import(
    pathToFileURL(resolve(realTransportModule)).href
  )) as {
    default?: McpTransport;
    transport?: McpTransport;
  };
  const transport = loaded.default ?? loaded.transport;
  if (transport === undefined) {
    throw new Error(
      "真实 MCP 传输模块必须导出 default 或 transport，并实现 McpTransport 接口",
    );
  }
  return transport;
}

afterEach(async () => {
  const { rm } = await import("node:fs/promises");
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true })),
  );
});

describe("CodebaseAdapter（MCP 传输）", () => {
  it("通过真实 MCP 传输完成索引与摘要读取", async () => {
    const root = await project();
    if (realTransportModule === undefined) {
      const result = await new CodebaseAdapter({
        isAvailable: async () => false,
      }).initialize(root);
      expect(result).toMatchObject({
        provider: "fallback-file-scan",
        degraded: true,
      });
      return;
    }
    const transport = await loadTransport();

    const result = await new CodebaseAdapter(transport).initialize(root);

    expect(result.provider).toBe("codebase-memory-mcp");
    expect(result.degraded).toBe(false);
    expect(result.codebaseSummary.length).toBeGreaterThan(0);
    expect(result.packageStructure.length).toBeGreaterThan(0);
    expect(result.architecture.length).toBeGreaterThan(0);
    expect(result.diagnostics).toMatchObject({
      installed: true,
      configured: true,
      connected: true,
      callable: true,
      indexed: true,
    });
  }, 60_000);
});
