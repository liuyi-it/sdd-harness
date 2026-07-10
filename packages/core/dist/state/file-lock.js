import { mkdir, open, readFile, unlink } from "node:fs/promises";
import { join } from "node:path";
import { SddError } from "../errors.js";
export class FileLock {
    root;
    ownsLock = false;
    path;
    constructor(root) {
        this.root = root;
        this.path = join(root, ".sdd", "lock");
    }
    async acquire(command, changeId, options = {}) {
        await mkdir(join(this.root, ".sdd"), { recursive: true });
        const deadline = typeof options.timeoutMs === "number" && options.timeoutMs > 0
            ? Date.now() + options.timeoutMs
            : undefined;
        await this.acquireUntil(command, changeId, options, deadline);
    }
    async acquireUntil(command, changeId, options, deadline) {
        const createdAt = new Date();
        const data = {
            pid: process.pid,
            command,
            ...(changeId === undefined ? {} : { changeId }),
            createdAt: createdAt.toISOString(),
            expiresAt: new Date(createdAt.getTime() + 10 * 60 * 1_000).toISOString(),
        };
        try {
            const handle = await open(this.path, "wx");
            await handle.writeFile(`${JSON.stringify(data)}\n`, "utf8");
            await handle.close();
            this.ownsLock = true;
            return;
        }
        catch (error) {
            if (!isAlreadyExists(error))
                throw error;
        }
        const existing = await this.readExisting();
        // 锁未过期，或者旧进程仍然存活，都说明当前不能继续写入。
        const unexpired = existing !== null &&
            Number.isFinite(Date.parse(existing.expiresAt)) &&
            Date.parse(existing.expiresAt) > Date.now();
        if (existing !== null && (unexpired || isProcessAlive(existing.pid))) {
            if (deadline !== undefined && Date.now() < deadline) {
                await delay(options.retryDelayMs ?? 50);
                return this.acquireUntil(command, changeId, options, deadline);
            }
            if (deadline !== undefined) {
                throw new SddError("E_LOCK_TIMEOUT", `命令 ${existing.command} 持有锁超时，无法在限定时间内获取写锁`, "sdd status");
            }
            throw new SddError("E_CONCURRENT_RUN", `命令 ${existing.command} 正在运行（pid ${existing.pid}）`, "sdd status");
        }
        await unlink(this.path).catch(() => undefined);
        return this.acquireUntil(command, changeId, options, deadline);
    }
    async release() {
        if (!this.ownsLock)
            return;
        await unlink(this.path).catch((error) => {
            if (!isMissingFile(error))
                throw error;
        });
        this.ownsLock = false;
    }
    async readExisting() {
        try {
            return JSON.parse(await readFile(this.path, "utf8"));
        }
        catch {
            return null;
        }
    }
}
function delay(milliseconds) {
    return new Promise((resolve) => setTimeout(resolve, milliseconds));
}
function isProcessAlive(pid) {
    try {
        process.kill(pid, 0);
        return true;
    }
    catch (error) {
        return !(typeof error === "object" &&
            error !== null &&
            "code" in error &&
            error.code === "ESRCH");
    }
}
function isAlreadyExists(error) {
    return (typeof error === "object" &&
        error !== null &&
        "code" in error &&
        error.code === "EEXIST");
}
function isMissingFile(error) {
    return (typeof error === "object" &&
        error !== null &&
        "code" in error &&
        error.code === "ENOENT");
}
//# sourceMappingURL=file-lock.js.map