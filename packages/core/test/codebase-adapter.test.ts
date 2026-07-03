import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  CodebaseAdapter,
  type McpTransport,
} from "../src/codebase/codebase-adapter.js";
import { PINNED_DEPENDENCIES } from "../src/dependencies.js";

const roots: string[] = [];

async function project(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "sdd-codebase-"));
  roots.push(root);
  await mkdir(join(root, "src/services"), { recursive: true });
  await writeFile(
    join(root, "src/services/order.ts"),
    "export class OrderService {}\n",
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
        version: "v0.8.1",
        commit: "f0c9be19c5d74b84f418d807bfdce7b5d6a261ff",
      },
      openSpec: {
        version: "v1.4.1",
        commit: "1b06fddd59d8e592d5b5794a1970b22867e85b1f",
      },
      superpowers: {
        version: "v6.1.1",
        commit: "d884ae04edebef577e82ff7c4e143debd0bbec99",
      },
    });
  });
});

describe("CodebaseAdapter", () => {
  it("indexes and reads summaries through an available MCP transport", async () => {
    const root = await project();
    const transport: McpTransport = {
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
    });
  });

  it("falls back to a bounded repository scan when MCP is unavailable", async () => {
    const root = await project();
    const transport: McpTransport = {
      isAvailable: vi.fn().mockResolvedValue(false),
      index: vi.fn(),
      summarize: vi.fn(),
    };

    const result = await new CodebaseAdapter(transport).initialize(root);

    expect(result.provider).toBe("fallback-file-scan");
    expect(result.degraded).toBe(true);
    expect(result.codebaseSummary).toContain("src/services/order.ts");
    expect(result.codebaseSummary).not.toContain("node_modules");
    expect(result.codebaseSummary).not.toContain("application-prod.yml");
    expect(result.codebaseSummary).not.toContain("server.pem");
  });
});
