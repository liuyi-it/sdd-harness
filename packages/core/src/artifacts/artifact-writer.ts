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
