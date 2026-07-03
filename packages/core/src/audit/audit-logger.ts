import {
  appendFile,
  mkdir,
  readFile,
  rename,
  stat,
  unlink,
} from "node:fs/promises";
import { dirname, join } from "node:path";

export interface AuditEvent {
  command: string;
  phase: string;
  result: "PASS" | "FAIL" | "PAUSED";
  message?: string;
  changeId?: string;
}

interface AuditOptions {
  maxBytes?: number;
  retainedFiles?: number;
}

export class AuditLogger {
  private readonly path: string;
  private readonly maxBytes: number;
  private readonly retainedFiles: number;

  constructor(root: string, options: AuditOptions = {}) {
    this.path = join(root, ".sdd", "logs", "audit.log");
    this.maxBytes = options.maxBytes ?? 10 * 1024 * 1024;
    this.retainedFiles = options.retainedFiles ?? 5;
  }

  async write(event: AuditEvent): Promise<void> {
    await mkdir(dirname(this.path), { recursive: true });
    await this.rotateIfRequired();
    const safeEvent = {
      timestamp: new Date().toISOString(),
      ...event,
      ...(event.message === undefined
        ? {}
        : { message: redact(event.message) }),
    };
    await appendFile(this.path, `${JSON.stringify(safeEvent)}\n`, "utf8");
  }

  async entries(): Promise<AuditEvent[]> {
    try {
      return (await readFile(this.path, "utf8"))
        .trim()
        .split("\n")
        .filter(Boolean)
        .map((line) => JSON.parse(line) as AuditEvent);
    } catch {
      return [];
    }
  }

  private async rotateIfRequired(): Promise<void> {
    try {
      if ((await stat(this.path)).size < this.maxBytes) return;
    } catch {
      return;
    }
    await unlink(`${this.path}.${this.retainedFiles}`).catch(() => undefined);
    for (let index = this.retainedFiles - 1; index >= 1; index -= 1) {
      await rename(`${this.path}.${index}`, `${this.path}.${index + 1}`).catch(
        () => undefined,
      );
    }
    await rename(this.path, `${this.path}.1`);
  }
}

function redact(value: string): string {
  return value.replace(
    /\b(token|password|secret|api[_-]?key|authorization)\s*[=:]\s*([^\s]+)/gi,
    "$1=[REDACTED]",
  );
}
