import { mkdir, open, readFile, unlink } from "node:fs/promises";
import { join } from "node:path";

import { SddError } from "../errors.js";

/**
 * FileLock 为所有写命令提供仓库级串行化保护。
 * 当锁文件过期时，会结合 pid 存活性决定是否允许抢占。
 */
interface LockData {
  pid: number;
  command: string;
  changeId?: string;
  createdAt: string;
  expiresAt: string;
}

interface AcquireOptions {
  timeoutMs?: number;
  retryDelayMs?: number;
}

export class FileLock {
  private ownsLock = false;
  private readonly path: string;

  constructor(readonly root: string) {
    this.path = join(root, ".sdd", "lock");
  }

  async acquire(
    command: string,
    changeId?: string,
    options: AcquireOptions = {},
  ): Promise<void> {
    await mkdir(join(this.root, ".sdd"), { recursive: true });
    const deadline =
      typeof options.timeoutMs === "number" && options.timeoutMs > 0
        ? Date.now() + options.timeoutMs
        : undefined;
    await this.acquireUntil(command, changeId, options, deadline);
  }

  private async acquireUntil(
    command: string,
    changeId: string | undefined,
    options: AcquireOptions,
    deadline: number | undefined,
  ): Promise<void> {
    const createdAt = new Date();
    const data: LockData = {
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
    } catch (error) {
      if (!isAlreadyExists(error)) throw error;
    }

    const existing = await this.readExisting();
    // 锁未过期，或者旧进程仍然存活，都说明当前不能继续写入。
    const unexpired =
      existing !== null &&
      Number.isFinite(Date.parse(existing.expiresAt)) &&
      Date.parse(existing.expiresAt) > Date.now();
    if (existing !== null && (unexpired || isProcessAlive(existing.pid))) {
      if (deadline !== undefined && Date.now() < deadline) {
        await delay(options.retryDelayMs ?? 50);
        return this.acquireUntil(command, changeId, options, deadline);
      }
      if (deadline !== undefined) {
        throw new SddError(
          "E_LOCK_TIMEOUT",
          `命令 ${existing.command} 持有锁超时，无法在限定时间内获取写锁`,
          "sdd status",
        );
      }
      throw new SddError(
        "E_CONCURRENT_RUN",
        `命令 ${existing.command} 正在运行（pid ${existing.pid}）`,
        "sdd status",
      );
    }
    await unlink(this.path).catch(() => undefined);
    return this.acquireUntil(command, changeId, options, deadline);
  }

  async release(): Promise<void> {
    if (!this.ownsLock) return;
    await unlink(this.path).catch((error: unknown) => {
      if (!isMissingFile(error)) throw error;
    });
    this.ownsLock = false;
  }

  private async readExisting(): Promise<LockData | null> {
    try {
      return JSON.parse(await readFile(this.path, "utf8")) as LockData;
    } catch {
      return null;
    }
  }
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return !(
      typeof error === "object" &&
      error !== null &&
      "code" in error &&
      error.code === "ESRCH"
    );
  }
}

function isAlreadyExists(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    error.code === "EEXIST"
  );
}

function isMissingFile(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    error.code === "ENOENT"
  );
}
