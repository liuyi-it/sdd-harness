import { createHash } from "node:crypto";
import {
  mkdir,
  open,
  readFile,
  rename,
  unlink,
  writeFile,
} from "node:fs/promises";
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
  constructor(
    private readonly renameFile: (
      source: string,
      target: string,
    ) => Promise<void> = rename,
  ) {}

  async writeGroupAtomically(
    artifacts: Array<{ path: string; content: string; inputs: unknown }>,
  ): Promise<void> {
    const nonce = `${process.pid}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    const prepared: Array<{ temporary: string; target: string }> = [];
    const backups: Array<{ backup: string; target: string }> = [];
    const committed: string[] = [];
    try {
      for (const artifact of artifacts) {
        const temporary = `${artifact.path}.tmp-${nonce}`;
        await this.write(temporary, artifact.content, artifact.inputs);
        await syncFile(temporary);
        await syncFile(`${temporary}.meta.json`);
        prepared.push({ temporary, target: artifact.path });
      }
      for (const item of prepared.flatMap(({ temporary, target }) => [
        { temporary, target },
        { temporary: `${temporary}.meta.json`, target: `${target}.meta.json` },
      ])) {
        const backup = `${item.target}.bak-${nonce}`;
        try {
          await this.renameFile(item.target, backup);
          await syncFile(backup);
          await syncDirectory(dirname(item.target));
          backups.push({ backup, target: item.target });
        } catch (error) {
          if (!isEnoent(error)) throw error;
        }
      }
      for (const item of prepared) {
        await this.renameFile(item.temporary, item.target);
        committed.push(item.target);
        await syncFile(item.target);
        await syncDirectory(dirname(item.target));
        await this.renameFile(
          `${item.temporary}.meta.json`,
          `${item.target}.meta.json`,
        );
        committed.push(`${item.target}.meta.json`);
        await syncFile(`${item.target}.meta.json`);
        await syncDirectory(dirname(item.target));
      }
      await Promise.all(backups.map(({ backup }) => unlink(backup)));
      await syncDirectories(backups.map(({ target }) => dirname(target)));
    } catch (error) {
      await Promise.all(
        committed.map((path) => unlink(path).catch(() => undefined)),
      );
      for (const { backup, target } of backups.reverse())
        await this.renameFile(backup, target);
      await Promise.all(
        prepared
          .flatMap((item) => [item.temporary, `${item.temporary}.meta.json`])
          .map((path) => unlink(path).catch(() => undefined)),
      );
      await syncDirectories(prepared.map(({ target }) => dirname(target)));
      throw error;
    }
  }

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
      const metadata: unknown = JSON.parse(metadataText);
      return (
        isArtifactMetadata(metadata) &&
        metadata.artifactHash === sha256(content)
      );
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

async function syncFile(path: string): Promise<void> {
  const handle = await open(path, "r");
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function syncDirectories(paths: string[]): Promise<void> {
  await Promise.all([...new Set(paths)].map(syncDirectory));
}

async function syncDirectory(path: string): Promise<void> {
  try {
    const handle = await open(path, "r");
    try {
      await handle.sync();
    } finally {
      await handle.close();
    }
  } catch {
    // Directory fsync is not supported by every Windows filesystem.
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
