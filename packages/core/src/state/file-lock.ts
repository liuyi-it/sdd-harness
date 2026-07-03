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

export class FileLock {
  private ownsLock = false;
  private readonly path: string;

  constructor(readonly root: string) {
    this.path = join(root, ".sdd", "lock");
  }

  async acquire(command: string, changeId?: string): Promise<void> {
    await mkdir(join(this.root, ".sdd"), { recursive: true });
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
      throw new SddError(
        "E_CONCURRENT_RUN",
        `命令 ${existing.command} 正在运行（pid ${existing.pid}）`,
        "sdd status",
      );
    }
    await unlink(this.path).catch(() => undefined);
    return this.acquire(command, changeId);
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
