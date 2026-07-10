export interface VerifyChecksumManifestInput {
    manifestContent: string;
    expectedManifestSha256: string;
    artifactName: string;
    artifactContent: Buffer;
}
export interface ComponentIntegrityEvidence {
    manifestContent: string;
    expectedManifestSha256?: string;
    artifactName?: string;
    targetPlatform?: "darwin" | "win32";
    targetArch?: "arm64" | "x64";
    artifactContentBase64: string;
}
/**
 * 校验第三方组件下载清单及目标产物摘要。
 * 先校验 checksums.txt 自身摘要，再校验目标平台产物条目。
 */
export declare function verifyChecksumManifest(input: VerifyChecksumManifestInput): void;
export declare function sha256Hex(input: string | Buffer): string;
export declare function decodeArtifactContent(base64: string): Buffer;
export declare function resolveCodebaseMemoryArtifactName(input?: {
    platform?: string;
    arch?: string;
}): string;
//# sourceMappingURL=dependency-integrity.d.ts.map