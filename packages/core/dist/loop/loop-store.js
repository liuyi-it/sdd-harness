import { mkdir, readFile, stat } from "node:fs/promises";
import { join } from "node:path";
import { ArtifactWriter } from "../artifacts/artifact-writer.js";
import { SddError } from "../errors.js";
/** runId 必须仅含安全字符，防止路径穿越攻击 */
function assertSafeRunId(runId) {
    if (!/^[a-zA-Z0-9_-]+$/.test(runId)) {
        throw new SddError("E_SECURITY_BLOCKED", `runId 包含非法字符：${runId}`);
    }
}
export class LoopStore {
    root;
    directory;
    specPath;
    runsDirectory;
    constructor(root) {
        this.root = root;
        this.directory = join(root, ".sdd", "loop");
        this.specPath = join(this.directory, "loop.json");
        this.runsDirectory = join(this.directory, "runs");
    }
    async writeSpec(spec) {
        await mkdir(this.runsDirectory, { recursive: true });
        await new ArtifactWriter().write(this.specPath, JSON.stringify(spec, null, 2), spec);
    }
    async readSpec() {
        return JSON.parse(await readFile(this.specPath, "utf8"));
    }
    async writeRun(run) {
        assertSafeRunId(run.runId);
        await mkdir(this.runsDirectory, { recursive: true });
        await new ArtifactWriter().write(join(this.runsDirectory, `${run.runId}.json`), JSON.stringify(run, null, 2), run);
    }
    async readRun(runId) {
        assertSafeRunId(runId);
        return JSON.parse(await readFile(join(this.runsDirectory, `${runId}.json`), "utf8"));
    }
    async hasRun(runId) {
        assertSafeRunId(runId);
        try {
            await stat(join(this.runsDirectory, `${runId}.json`));
            return true;
        }
        catch {
            return false;
        }
    }
}
//# sourceMappingURL=loop-store.js.map