import { Core, HostAdapter, type SddCore } from "@sdd-harness/core";

export class ClaudeCodeAdapter extends HostAdapter {
  constructor(core: SddCore = new Core()) {
    super(core, "claude-code");
  }
}
