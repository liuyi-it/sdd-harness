import { mkdir, readFile, stat } from "node:fs/promises";
import { join } from "node:path";

import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import type { LoopRun, LoopSpec } from "./model.js";

export class LoopStore {
  readonly directory: string;
  readonly specPath: string;
  readonly runsDirectory: string;

  constructor(private readonly root: string) {
    this.directory = join(root, ".sdd", "loop");
    this.specPath = join(this.directory, "loop.json");
    this.runsDirectory = join(this.directory, "runs");
  }

  async writeSpec(spec: LoopSpec): Promise<void> {
    await mkdir(this.runsDirectory, { recursive: true });
    await new ArtifactWriter().write(
      this.specPath,
      JSON.stringify(spec, null, 2),
      spec,
    );
  }

  async readSpec(): Promise<LoopSpec> {
    return JSON.parse(await readFile(this.specPath, "utf8")) as LoopSpec;
  }

  async writeRun(run: LoopRun): Promise<void> {
    await mkdir(this.runsDirectory, { recursive: true });
    await new ArtifactWriter().write(
      join(this.runsDirectory, `${run.runId}.json`),
      JSON.stringify(run, null, 2),
      run,
    );
  }

  async readRun(runId: string): Promise<LoopRun> {
    return JSON.parse(
      await readFile(join(this.runsDirectory, `${runId}.json`), "utf8"),
    ) as LoopRun;
  }

  async hasRun(runId: string): Promise<boolean> {
    try {
      await stat(join(this.runsDirectory, `${runId}.json`));
      return true;
    } catch {
      return false;
    }
  }
}
