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

export interface CodexRuntimeOptions {
  taskExecutor: TaskExecutor;
  mcpTransport?: McpTransport;
  specEngine?: SpecEngine;
  tddEngine?: TddEngine;
}

export class CodexAdapter extends HostAdapter {
  constructor(coreOrOptions: SddCore | CodexRuntimeOptions = new Core()) {
    super(resolveCore(coreOrOptions), "codex");
  }
}

function resolveCore(coreOrOptions: SddCore | CodexRuntimeOptions): SddCore {
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
