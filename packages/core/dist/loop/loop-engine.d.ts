import type { CommandRequest, CommandResult } from "../contracts.js";
import type { StateStore } from "../state/state-store.js";
import type { LoopStore } from "./loop-store.js";
import type { LoopEventStore } from "./loop-events.js";
export declare class LoopEngine {
    private readonly root;
    private readonly store;
    private readonly loops;
    private readonly events;
    private readonly execute;
    constructor(root: string, store: StateStore, loops: LoopStore, events: LoopEventStore, execute: (req: CommandRequest) => Promise<CommandResult>);
    run(request: CommandRequest): Promise<CommandResult>;
    private runAuto;
    resumeAuto(request: CommandRequest): Promise<CommandResult>;
    restartAuto(request: CommandRequest): Promise<CommandResult>;
    stopAuto(): Promise<CommandResult>;
    getEvents(request: CommandRequest): Promise<CommandResult>;
    getLoopStatus(): Promise<CommandResult>;
    private autoCommand;
    private prepareLoop;
    private recordStep;
    private finalizeLoop;
    private readLoopSpec;
    private recoverHistoricalHandoff;
}
//# sourceMappingURL=loop-engine.d.ts.map