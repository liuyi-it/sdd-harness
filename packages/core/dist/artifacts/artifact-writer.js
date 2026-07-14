import { createHash } from "node:crypto";
import { access, mkdir, open, readFile, rename, unlink, writeFile, } from "node:fs/promises";
import { dirname, join, relative } from "node:path";
const MANIFEST_NAME = "artifacts.json";
const manifestQueues = new Map();
/** 所有制品摘要集中写入 `.sdd/artifacts.json`，避免为每个文件生成 sidecar。 */
export class ArtifactWriter {
    renameFile;
    constructor(renameFile = rename) {
        this.renameFile = renameFile;
    }
    async writeGroupAtomically(artifacts) {
        if (artifacts.length === 0)
            return;
        const nonce = nonceValue();
        const prepared = await Promise.all(artifacts.map(async (artifact) => {
            const normalized = normalizeContent(artifact.content);
            const temporary = `${artifact.path}.tmp-${nonce}`;
            await mkdir(dirname(artifact.path), { recursive: true });
            await writeFile(temporary, normalized, "utf8");
            await syncFile(temporary);
            return {
                temporary,
                target: artifact.path,
                metadata: createMetadata(normalized, artifact.inputs),
            };
        }));
        const backups = [];
        const committed = [];
        try {
            for (const item of prepared) {
                const backup = `${item.target}.bak-${nonce}`;
                try {
                    await this.renameFile(item.target, backup);
                    backups.push({ backup, target: item.target });
                }
                catch (error) {
                    if (!isEnoent(error))
                        throw error;
                }
            }
            for (const item of prepared) {
                await this.renameFile(item.temporary, item.target);
                committed.push(item.target);
                await syncFile(item.target);
                await syncDirectory(dirname(item.target));
            }
            await this.writeMetadataEntries(prepared.map(({ target, metadata }) => ({ path: target, metadata })));
            await Promise.all(backups.map(({ backup }) => unlink(backup)));
            await syncDirectories(backups.map(({ target }) => dirname(target)));
        }
        catch (error) {
            await Promise.all(committed.map((path) => unlink(path).catch(() => undefined)));
            for (const { backup, target } of backups.reverse())
                await this.renameFile(backup, target);
            await Promise.all(prepared.map(({ temporary }) => unlink(temporary).catch(() => undefined)));
            await syncDirectories(prepared.map(({ target }) => dirname(target)));
            throw error;
        }
    }
    async write(path, content, inputs) {
        await mkdir(dirname(path), { recursive: true });
        const normalized = normalizeContent(content);
        const metadata = createMetadata(normalized, inputs);
        const temporary = `${path}.tmp-${nonceValue()}`;
        await writeFile(temporary, normalized, "utf8");
        await this.renameFile(temporary, path);
        await this.writeMetadataEntries([{ path, metadata }]);
        return metadata;
    }
    async metadata(path) {
        try {
            const manifestPath = await resolveManifestPath(path);
            const manifest = parseManifest(await readFile(manifestPath, "utf8"));
            return manifest.artifacts[manifestKey(manifestPath, path)];
        }
        catch {
            return undefined;
        }
    }
    async isUnmodified(path) {
        try {
            const [content, metadata] = await Promise.all([
                readFile(path, "utf8"),
                this.metadata(path),
            ]);
            return (metadata !== undefined && metadata.artifactHash === sha256(content));
        }
        catch {
            return false;
        }
    }
    async writeMetadataEntries(entries) {
        const grouped = new Map();
        for (const entry of entries) {
            const manifestPath = await resolveManifestPath(entry.path);
            const current = grouped.get(manifestPath) ?? [];
            current.push(entry);
            grouped.set(manifestPath, current);
        }
        for (const [manifestPath, manifestEntries] of grouped) {
            await enqueueManifestWrite(manifestPath, async () => {
                const manifest = await readManifest(manifestPath);
                for (const entry of manifestEntries)
                    manifest.artifacts[manifestKey(manifestPath, entry.path)] =
                        entry.metadata;
                await writeJsonAtomically(manifestPath, manifest, this.renameFile);
            });
        }
    }
}
async function resolveManifestPath(path) {
    let current = dirname(path);
    while (true) {
        const sddRoot = join(current, ".sdd");
        try {
            await access(sddRoot);
            return join(sddRoot, MANIFEST_NAME);
        }
        catch {
            const parent = dirname(current);
            if (parent === current)
                break;
            current = parent;
        }
    }
    return join(dirname(path), MANIFEST_NAME);
}
function manifestKey(manifestPath, path) {
    return relative(dirname(manifestPath), path).replaceAll("\\", "/");
}
async function readManifest(path) {
    try {
        return parseManifest(await readFile(path, "utf8"));
    }
    catch {
        return { schemaVersion: "1.0.0", artifacts: {} };
    }
}
function parseManifest(content) {
    const value = JSON.parse(content);
    if (value.schemaVersion !== "1.0.0" ||
        typeof value.artifacts !== "object" ||
        value.artifacts === null)
        throw new Error("制品摘要清单结构无效");
    return value;
}
async function enqueueManifestWrite(path, operation) {
    const previous = manifestQueues.get(path) ?? Promise.resolve();
    const current = previous.catch(() => undefined).then(operation);
    manifestQueues.set(path, current);
    try {
        await current;
    }
    finally {
        if (manifestQueues.get(path) === current)
            manifestQueues.delete(path);
    }
}
async function writeJsonAtomically(path, value, renameFile) {
    await mkdir(dirname(path), { recursive: true });
    const temporary = `${path}.tmp-${nonceValue()}`;
    await writeFile(temporary, `${JSON.stringify(value, null, 2)}\n`, "utf8");
    await syncFile(temporary);
    await renameFile(temporary, path);
    await syncDirectory(dirname(path));
}
function createMetadata(content, inputs) {
    return {
        schemaVersion: "1.0.0",
        generatedBy: "sdd-harness",
        inputHash: sha256(stableStringify(inputs)),
        artifactHash: sha256(content),
        createdAt: new Date().toISOString(),
    };
}
function normalizeContent(content) {
    return content.endsWith("\n") ? content : `${content}\n`;
}
function nonceValue() {
    return `${process.pid}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
async function syncFile(path) {
    const handle = await open(path, "r");
    try {
        await handle.sync();
    }
    finally {
        await handle.close();
    }
}
async function syncDirectories(paths) {
    await Promise.all([...new Set(paths)].map(syncDirectory));
}
async function syncDirectory(path) {
    try {
        const handle = await open(path, "r");
        try {
            await handle.sync();
        }
        finally {
            await handle.close();
        }
    }
    catch {
        // 部分 Windows 文件系统不支持目录 fsync。
    }
}
function isEnoent(error) {
    return (error instanceof Error &&
        "code" in error &&
        error.code === "ENOENT");
}
function sha256(value) {
    return `sha256:${createHash("sha256").update(value).digest("hex")}`;
}
function stableStringify(value) {
    if (Array.isArray(value))
        return `[${value.map(stableStringify).join(",")}]`;
    if (value !== null && typeof value === "object") {
        return `{${Object.entries(value)
            .sort(([left], [right]) => left.localeCompare(right))
            .map(([key, entry]) => `${JSON.stringify(key)}:${stableStringify(entry)}`)
            .join(",")}}`;
    }
    return JSON.stringify(value);
}
export function artifactInputHash(value) {
    return sha256(stableStringify(value));
}
//# sourceMappingURL=artifact-writer.js.map