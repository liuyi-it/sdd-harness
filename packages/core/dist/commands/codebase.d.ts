import type { CodebaseAdapter } from "../codebase/codebase-adapter.js";
import type { CommandResult } from "../contracts.js";
/** codebase 子命令分发：status / doctor / index / query / rebuild */
export declare function runCodebaseCommand(root: string, codebase: CodebaseAdapter, args: Record<string, unknown> | undefined): Promise<CommandResult>;
//# sourceMappingURL=codebase.d.ts.map