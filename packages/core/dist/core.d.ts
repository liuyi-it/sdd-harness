import { CodebaseAdapter } from "./codebase/codebase-adapter.js";
import { type CommandRequest, type CommandResult, type SddCore } from "./contracts.js";
import { SpecEngine } from "./engines/spec/spec-engine.js";
import { TddEngine } from "./engines/tdd/tdd-engine.js";
import { type TaskExecutor } from "./build/task-executor.js";
/**
 * Core 是整个工作流的统一调度入口。
 * 所有平台适配器最终都只通过这里推进状态机、写入制品和返回结果。
 */
interface CoreDependencies {
    codebase?: CodebaseAdapter;
    specEngine?: SpecEngine;
    tddEngine?: TddEngine;
    taskExecutor?: TaskExecutor;
}
export declare class Core implements SddCore {
    private readonly codebase;
    private readonly specEngine;
    private readonly tddEngine;
    private readonly taskExecutor;
    constructor(dependencies?: CoreDependencies);
    execute(request: CommandRequest): Promise<CommandResult>;
    private runAuto;
}
export {};
//# sourceMappingURL=core.d.ts.map