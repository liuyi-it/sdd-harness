import { Core, HostAdapter, type SddCore } from "@sdd-harness/core";

export class CodexAdapter extends HostAdapter {
  constructor(core: SddCore = new Core()) {
    super(core, "codex");
  }
}
