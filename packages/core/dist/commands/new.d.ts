import { type CodebaseAdapter } from "../codebase/codebase-adapter.js";
import { type CommandResult } from "../contracts.js";
import type { SpecEngine } from "../engines/spec/spec-engine.js";
export declare function runNew(root: string, rawArgs: Record<string, unknown> | undefined, engine: SpecEngine, codebase?: CodebaseAdapter, signal?: AbortSignal): Promise<CommandResult>;
//# sourceMappingURL=new.d.ts.map