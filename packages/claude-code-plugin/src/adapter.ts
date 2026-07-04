import {
  CodebaseAdapter,
  Core,
  HostAdapter,
  type McpTransport,
  type SddCore,
  type SpecEngine,
  type TaskExecutor,
  type TddEngine,
} from "@sdd-harness/core";

export interface ClaudeCodeRuntimeOptions {
  taskExecutor: TaskExecutor;
  mcpTransport?: McpTransport;
  specEngine?: SpecEngine;
  tddEngine?: TddEngine;
}

export class ClaudeCodeAdapter extends HostAdapter {
  constructor(coreOrOptions: SddCore | ClaudeCodeRuntimeOptions = new Core()) {
    super(resolveCore(coreOrOptions), "claude-code");
  }
}

function resolveCore(
  coreOrOptions: SddCore | ClaudeCodeRuntimeOptions,
): SddCore {
  return "execute" in coreOrOptions
    ? coreOrOptions
    : new Core({
        codebase: new CodebaseAdapter(coreOrOptions.mcpTransport),
        taskExecutor: coreOrOptions.taskExecutor,
        ...(coreOrOptions.specEngine === undefined
          ? {}
          : { specEngine: coreOrOptions.specEngine }),
        ...(coreOrOptions.tddEngine === undefined
          ? {}
          : { tddEngine: coreOrOptions.tddEngine }),
      });
}
