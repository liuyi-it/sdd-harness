import { appendFile, mkdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { SddError } from "../errors.js";
/** Loop 事件存储：JSONL 格式 */
export class LoopEventStore {
    root;
    eventsDirectory;
    constructor(root) {
        this.root = root;
        this.eventsDirectory = join(root, ".sdd", "loop", "events");
    }
    async write(runId, event) {
        assertSafeRunId(runId);
        await mkdir(this.eventsDirectory, { recursive: true });
        const fullEvent = {
            schemaVersion: "1.0.0",
            eventId: randomUUID(),
            createdAt: new Date().toISOString(),
            ...event,
        };
        await appendFile(join(this.eventsDirectory, `${runId}.jsonl`), `${JSON.stringify(fullEvent)}\n`, "utf8");
    }
    async read(runId, opts) {
        assertSafeRunId(runId);
        try {
            const content = await readFile(join(this.eventsDirectory, `${runId}.jsonl`), "utf8");
            const lines = content.trim().split("\n").filter(Boolean);
            const events = lines.map((line) => JSON.parse(line));
            if (opts?.tail !== undefined && opts.tail > 0) {
                return events.slice(-opts.tail);
            }
            return events;
        }
        catch {
            return [];
        }
    }
}
/** runId 必须仅含安全字符，防止路径穿越攻击 */
function assertSafeRunId(runId) {
    if (!/^[a-zA-Z0-9_-]+$/.test(runId)) {
        throw new SddError("E_SECURITY_BLOCKED", `runId 包含非法字符：${runId}`);
    }
}
//# sourceMappingURL=loop-events.js.map