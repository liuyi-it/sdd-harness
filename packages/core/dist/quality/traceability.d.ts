import type { TaskDefinition } from "../engines/tdd/tdd-engine.js";
import type { SpecDocument } from "../engines/openspec/model.js";
import type { StoredTaskResult } from "./quality-gates.js";
export declare function readAuthoritativeSpec(change: string, markdown: string): Promise<{
    document: SpecDocument;
    legacy: boolean;
}>;
export declare function traceabilityFailures(document: SpecDocument, tasks: TaskDefinition[], results: StoredTaskResult[], requireArtifacts?: boolean): string[];
export declare function renderTraceability(document: SpecDocument, tasks: TaskDefinition[], results: StoredTaskResult[]): string;
//# sourceMappingURL=traceability.d.ts.map