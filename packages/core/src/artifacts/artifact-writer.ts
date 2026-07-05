import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

/**
 * ArtifactWriter 统一负责阶段制品和元数据落盘。
 * 这样各命令只需要关心内容本身，不必重复实现摘要与 candidate 逻辑。
 */
export interface ArtifactMetadata {
  schemaVersion: "1.0.0";
  generatedBy: "sdd-harness";
  inputHash: string;
  artifactHash: string;
  createdAt: string;
}

export class ArtifactWriter {
  async write(
    path: string,
    content: string,
    inputs: unknown,
  ): Promise<ArtifactMetadata> {
    await mkdir(dirname(path), { recursive: true });
    const normalized = content.endsWith("\n") ? content : `${content}\n`;
    const metadata: ArtifactMetadata = {
      schemaVersion: "1.0.0",
      generatedBy: "sdd-harness",
      inputHash: sha256(stableStringify(inputs)),
      artifactHash: sha256(normalized),
      createdAt: new Date().toISOString(),
    };
    await writeFile(path, normalized, "utf8");
    await writeFile(
      `${path}.meta.json`,
      `${JSON.stringify(metadata, null, 2)}\n`,
      "utf8",
    );
    return metadata;
  }

  async isUnmodified(path: string): Promise<boolean> {
    try {
      const [content, metadataText] = await Promise.all([
        readFile(path, "utf8"),
        readFile(`${path}.meta.json`, "utf8"),
      ]);
      const metadata = JSON.parse(metadataText) as ArtifactMetadata;
      return metadata.artifactHash === sha256(content);
    } catch {
      return false;
    }
  }

  async writeOrCandidate(
    path: string,
    content: string,
    inputs: unknown,
    options: { force?: boolean } = {},
  ): Promise<"written" | "unchanged" | "candidate"> {
    if (options.force === true) {
      await this.write(path, content, inputs);
      return "written";
    }
    try {
      const [currentContent, metadataText] = await Promise.all([
        readFile(path, "utf8"),
        readFile(`${path}.meta.json`, "utf8"),
      ]);
      const metadata = JSON.parse(metadataText) as ArtifactMetadata;
      if (
        metadata.inputHash === artifactInputHash(inputs) &&
        metadata.artifactHash === sha256(currentContent)
      ) {
        return "unchanged";
      }
      await this.write(`${path}.candidate.md`, content, inputs);
      return "candidate";
    } catch {
      await this.write(path, content, inputs);
      return "written";
    }
  }

  async writeGroupOrCandidates(
    artifacts: Array<{ path: string; content: string }>,
    inputs: unknown,
    options: { force?: boolean } = {},
  ): Promise<"written" | "unchanged" | "candidate"> {
    if (options.force === true) {
      await Promise.all(
        artifacts.map((artifact) =>
          this.write(artifact.path, artifact.content, inputs),
        ),
      );
      return "written";
    }
    const states = await Promise.all(
      artifacts.map(async (artifact) => {
        let content: string;
        try {
          content = await readFile(artifact.path, "utf8");
        } catch (error) {
          if (!isEnoent(error)) throw error;
          return { kind: "missing" as const, sameInput: true };
        }
        let metadataText: string;
        try {
          metadataText = await readFile(`${artifact.path}.meta.json`, "utf8");
        } catch (error) {
          if (!isEnoent(error)) throw error;
          return { kind: "modified" as const, sameInput: false };
        }
        let metadata: ArtifactMetadata;
        try {
          const parsed: unknown = JSON.parse(metadataText);
          if (!isArtifactMetadata(parsed))
            return { kind: "modified" as const, sameInput: false };
          metadata = parsed;
        } catch (error) {
          if (!(error instanceof SyntaxError)) throw error;
          return { kind: "modified" as const, sameInput: false };
        }
        return {
          kind:
            metadata.artifactHash === sha256(content)
              ? ("unmodified" as const)
              : ("modified" as const),
          sameInput: metadata.inputHash === artifactInputHash(inputs),
        };
      }),
    );
    const requiresCandidates = states.some(
      (state) => state.kind === "modified" || !state.sameInput,
    );
    if (requiresCandidates) {
      await Promise.all(
        artifacts.map((artifact) =>
          this.write(`${artifact.path}.candidate.md`, artifact.content, inputs),
        ),
      );
      return "candidate";
    }
    const missing = artifacts.filter(
      (_, index) => states[index]!.kind === "missing",
    );
    if (missing.length === 0) return "unchanged";
    await Promise.all(
      missing.map((artifact) =>
        this.write(artifact.path, artifact.content, inputs),
      ),
    );
    return "written";
  }
}

function isEnoent(error: unknown): boolean {
  return (
    error instanceof Error &&
    "code" in error &&
    (error as NodeJS.ErrnoException).code === "ENOENT"
  );
}

function isArtifactMetadata(value: unknown): value is ArtifactMetadata {
  if (typeof value !== "object" || value === null) return false;
  const metadata = value as Partial<ArtifactMetadata>;
  return (
    metadata.schemaVersion === "1.0.0" &&
    metadata.generatedBy === "sdd-harness" &&
    typeof metadata.inputHash === "string" &&
    /^sha256:[a-f0-9]{64}$/.test(metadata.inputHash) &&
    typeof metadata.artifactHash === "string" &&
    /^sha256:[a-f0-9]{64}$/.test(metadata.artifactHash) &&
    typeof metadata.createdAt === "string"
  );
}

function sha256(value: string): string {
  return `sha256:${createHash("sha256").update(value).digest("hex")}`;
}

function stableStringify(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(",")}]`;
  if (value !== null && typeof value === "object") {
    return `{${Object.entries(value)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, entry]) => `${JSON.stringify(key)}:${stableStringify(entry)}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

export function artifactInputHash(value: unknown): string {
  return sha256(stableStringify(value));
}
