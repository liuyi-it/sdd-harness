import type { Phase } from "../contracts.js";
import type { LoopDecision } from "./model.js";
/** Loop 事件类型 */
export type LoopEventType = "LOOP_STARTED" | "LOOP_RESUMED" | "LOOP_STOPPED" | "LOOP_RESTARTED" | "COMMAND_STARTED" | "COMMAND_FINISHED" | "ACTION_REQUIRED" | "TASK_COMPLETED" | "DECISION_MADE" | "STATE_CONVERGED" | "LOOP_PAUSED" | "LOOP_FAILED" | "LOOP_ARCHIVED";
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
export declare class LoopEventStore {
    private readonly root;
    readonly eventsDirectory: string;
    constructor(root: string);
    write(runId: string, event: Omit<LoopEvent, "eventId" | "schemaVersion" | "createdAt">): Promise<void>;
    read(runId: string, opts?: {
        tail?: number;
    }): Promise<LoopEvent[]>;
}
//# sourceMappingURL=loop-events.d.ts.map