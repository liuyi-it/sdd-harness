import { createHash } from "node:crypto";
import { SddError } from "./errors.js";
/**
 * 校验第三方组件下载清单及目标产物摘要。
 * 先校验 checksums.txt 自身摘要，再校验目标平台产物条目。
 */
export function verifyChecksumManifest(input) {
    const manifestSha256 = sha256Hex(input.manifestContent);
    if (manifestSha256 !== input.expectedManifestSha256) {
        throw new SddError("E_COMPONENT_INTEGRITY_FAILED", `校验清单摘要不匹配：期望 ${input.expectedManifestSha256}，实际 ${manifestSha256}`);
    }
    const expectedArtifactSha256 = manifestEntry(input.manifestContent, input.artifactName);
    const actualArtifactSha256 = sha256Hex(input.artifactContent);
    if (expectedArtifactSha256 !== actualArtifactSha256) {
        throw new SddError("E_COMPONENT_INTEGRITY_FAILED", `组件产物摘要不匹配：${input.artifactName}`);
    }
}
export function sha256Hex(input) {
    return createHash("sha256").update(input).digest("hex");
}
export function decodeArtifactContent(base64) {
    return Buffer.from(base64, "base64");
}
export function resolveCodebaseMemoryArtifactName(input) {
    const platform = input?.platform ?? process.platform;
    const arch = input?.arch ?? process.arch;
    if ((platform !== "darwin" && platform !== "win32") ||
        (arch !== "arm64" && arch !== "x64")) {
        throw new SddError("E_COMPONENT_INTEGRITY_FAILED", `不支持的平台或架构：${platform}/${arch}`);
    }
    return `codebase-memory-mcp-${platform}-${arch}.tar.gz`;
}
function manifestEntry(manifestContent, artifactName) {
    for (const line of manifestContent.split(/\r?\n/)) {
        const trimmed = line.trim();
        if (trimmed.length === 0)
            continue;
        const match = trimmed.match(/^([a-f0-9]{6,})\s+\*?(.+)$/i);
        if (match?.[2] === artifactName)
            return match[1].toLowerCase();
    }
    throw new SddError("E_COMPONENT_INTEGRITY_FAILED", `校验清单中缺少目标产物：${artifactName}`);
}
//# sourceMappingURL=dependency-integrity.js.map