import { appendFile, mkdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import type { Phase } from "../contracts.js";
import type { LoopDecision } from "./model.js";

/** Loop 事件类型 */
export type LoopEventType =
  | "LOOP_STARTED"
  | "LOOP_RESUMED"
  | "LOOP_STOPPED"
  | "LOOP_RESTARTED"
  | "COMMAND_STARTED"
  | "COMMAND_FINISHED"
  | "ACTION_REQUIRED"
  | "TASK_COMPLETED"
  | "DECISION_MADE"
  | "STATE_CONVERGED"
  | "LOOP_PAUSED"
  | "LOOP_FAILED"
  | "LOOP_ARCHIVED";

/** Loop 事件结构 */
export interface LoopEvent {
  schemaVersion: "1.0.0";
  eventId: string;
  loopId: string;
  runId: string;
  type: LoopEventType;
  phase?: Phase;
  command?: string;
  taskId?: string;
  decision?: LoopDecision;
  data?: Record<string, unknown>;
  createdAt: string;
}

/** Loop 事件存储：JSONL 格式 */
export class LoopEventStore {
  readonly eventsDirectory: string;

  constructor(private readonly root: string) {
    this.eventsDirectory = join(root, ".sdd", "loop", "events");
  }

  async write(
    runId: string,
    event: Omit<LoopEvent, "eventId" | "schemaVersion" | "createdAt">,
  ): Promise<void> {
    await mkdir(this.eventsDirectory, { recursive: true });
    const fullEvent: LoopEvent = {
      schemaVersion: "1.0.0",
      eventId: randomUUID(),
      createdAt: new Date().toISOString(),
      ...event,
    };
    await appendFile(
      join(this.eventsDirectory, `${runId}.jsonl`),
      `${JSON.stringify(fullEvent)}\n`,
      "utf8",
    );
  }

  async read(
    runId: string,
    opts?: { tail?: number },
  ): Promise<LoopEvent[]> {
    try {
      const content = await readFile(
        join(this.eventsDirectory, `${runId}.jsonl`),
        "utf8",
      );
      const lines = content.trim().split("\n").filter(Boolean);
      const events = lines.map(
        (line) => JSON.parse(line) as LoopEvent,
      );
      if (opts?.tail !== undefined && opts.tail > 0) {
        return events.slice(-opts.tail);
      }
      return events;
    } catch {
      return [];
    }
  }
}
