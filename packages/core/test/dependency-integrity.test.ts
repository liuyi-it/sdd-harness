import { describe, expect, it } from "vitest";

import {
  decodeArtifactContent,
  resolveCodebaseMemoryArtifactName,
  sha256Hex,
  verifyChecksumManifest,
} from "../src/dependency-integrity.js";

describe("dependency integrity", () => {
  it("校验通过时允许继续使用产物", () => {
    const artifact = Buffer.from("archive-bytes");
    const artifactSha256 = sha256Hex(artifact);
    const finalManifest = `${artifactSha256}  codebase-memory-mcp-darwin-arm64.tar.gz\n`;
    const manifestSha256 = sha256Hex(finalManifest);

    expect(() =>
      verifyChecksumManifest({
        manifestContent: finalManifest,
        expectedManifestSha256: manifestSha256,
        artifactName: "codebase-memory-mcp-darwin-arm64.tar.gz",
        artifactContent: artifact,
      }),
    ).not.toThrow();
  });

  it("当 manifest 自身摘要不匹配时返回 E_COMPONENT_INTEGRITY_FAILED", () => {
    try {
      verifyChecksumManifest({
        manifestContent: "abc123  codebase-memory-mcp-darwin-arm64.tar.gz\n",
        expectedManifestSha256: "mismatch",
        artifactName: "codebase-memory-mcp-darwin-arm64.tar.gz",
        artifactContent: Buffer.from("archive-bytes"),
      });
      expect.unreachable("应当抛出完整性校验错误");
    } catch (error) {
      expect(error).toMatchObject({
        code: "E_COMPONENT_INTEGRITY_FAILED",
      });
    }
  });

  it("当产物摘要不匹配时返回 E_COMPONENT_INTEGRITY_FAILED", () => {
    const manifest = "deadbeef  codebase-memory-mcp-darwin-arm64.tar.gz\n";
    const manifestSha256 = sha256Hex(manifest);

    try {
      verifyChecksumManifest({
        manifestContent: manifest,
        expectedManifestSha256: manifestSha256,
        artifactName: "codebase-memory-mcp-darwin-arm64.tar.gz",
        artifactContent: Buffer.from("archive-bytes"),
      });
      expect.unreachable("应当抛出完整性校验错误");
    } catch (error) {
      expect(error).toMatchObject({
        code: "E_COMPONENT_INTEGRITY_FAILED",
      });
    }
  });

  it("可以把 base64 安装产物还原为 Buffer", () => {
    const content = Buffer.from("archive-bytes");

    expect(decodeArtifactContent(content.toString("base64"))).toEqual(content);
  });

  it("可以根据 macOS 或 Windows 平台自动推导产物名", () => {
    expect(
      resolveCodebaseMemoryArtifactName({ platform: "darwin", arch: "arm64" }),
    ).toBe("codebase-memory-mcp-darwin-arm64.tar.gz");
    expect(
      resolveCodebaseMemoryArtifactName({ platform: "win32", arch: "x64" }),
    ).toBe("codebase-memory-mcp-win32-x64.tar.gz");
  });
});
