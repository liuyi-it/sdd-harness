import { type CommandResult } from "../contracts.js";
import type { TddEngine } from "../engines/tdd/tdd-engine.js";
/**
 * design 阶段把 spec、impact 和代码库上下文收束成可执行设计稿。
 * 如果输入未变化，则直接返回 already ready，保持幂等。
 */
export declare function runDesign(root: string, engine: TddEngine, args?: Record<string, unknown>, signal?: AbortSignal): Promise<CommandResult>;
//# sourceMappingURL=design.d.ts.map