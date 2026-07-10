import type { LoopRun, LoopSpec } from "./model.js";
export declare class LoopStore {
    private readonly root;
    readonly directory: string;
    readonly specPath: string;
    readonly runsDirectory: string;
    constructor(root: string);
    writeSpec(spec: LoopSpec): Promise<void>;
    readSpec(): Promise<LoopSpec>;
    writeRun(run: LoopRun): Promise<void>;
    readRun(runId: string): Promise<LoopRun>;
    hasRun(runId: string): Promise<boolean>;
}
//# sourceMappingURL=loop-store.d.ts.map