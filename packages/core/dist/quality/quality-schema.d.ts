import type { StoredTaskResult } from "./quality-gates.js";
import type { TaskDefinition } from "../engines/tdd/tdd-engine.js";
export declare function parseTasks(raw: string): TaskDefinition[];
export declare function assertTaskResultIds(tasks: TaskDefinition[], results: StoredTaskResult[]): void;
export declare function parseTaskResults(raw: string): StoredTaskResult[];
//# sourceMappingURL=quality-schema.d.ts.map