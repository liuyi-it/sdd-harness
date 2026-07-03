import { CodebaseAdapter } from "./codebase/codebase-adapter.js";
import { runInit } from "./commands/init.js";
import { runNew } from "./commands/new.js";
import { runDesign } from "./commands/design.js";
import { runPlan } from "./commands/plan.js";
import { runBuild } from "./commands/build.js";
import { runVerify } from "./commands/verify.js";
import { runReview } from "./commands/review.js";
import { runArchive } from "./commands/archive.js";
import { runStatus } from "./commands/status.js";
import {
  type CommandRequest,
  type CommandResult,
  type SddCore,
} from "./contracts.js";
import { SddError } from "./errors.js";
import { SpecEngine } from "./engines/spec/spec-engine.js";
import { TddEngine } from "./engines/tdd/tdd-engine.js";
import {
  MissingTaskExecutor,
  type TaskExecutor,
} from "./build/task-executor.js";

interface CoreDependencies {
  codebase?: CodebaseAdapter;
  specEngine?: SpecEngine;
  tddEngine?: TddEngine;
  taskExecutor?: TaskExecutor;
}

export class Core implements SddCore {
  private readonly codebase: CodebaseAdapter;
  private readonly specEngine: SpecEngine;
  private readonly tddEngine: TddEngine;
  private readonly taskExecutor: TaskExecutor;

  constructor(dependencies: CoreDependencies = {}) {
    this.codebase = dependencies.codebase ?? new CodebaseAdapter();
    this.specEngine = dependencies.specEngine ?? new SpecEngine();
    this.tddEngine = dependencies.tddEngine ?? new TddEngine();
    this.taskExecutor = dependencies.taskExecutor ?? new MissingTaskExecutor();
  }

  async execute(request: CommandRequest): Promise<CommandResult> {
    try {
      if (request.command === "status") return await runStatus(request.cwd);
      if (request.command === "init")
        return await runInit(request.cwd, this.codebase);
      const current = await runStatus(request.cwd);
      if (
        current.state === "ARCHIVED" &&
        request.command !== "archive" &&
        request.command !== "new"
      ) {
        throw new SddError(
          "E_ARCHIVED_READONLY",
          "Archived changes are read-only",
        );
      }
      if (request.command === "auto") return await this.runAuto(request);
      if (request.command === "new")
        return await runNew(request.cwd, request.args, this.specEngine);
      if (request.command === "design")
        return await runDesign(request.cwd, this.tddEngine);
      if (request.command === "plan")
        return await runPlan(request.cwd, this.tddEngine);
      if (request.command === "build")
        return await runBuild(
          request.cwd,
          this.taskExecutor,
          request.signal,
          request.args,
        );
      if (request.command === "verify") return await runVerify(request.cwd);
      if (request.command === "review") return await runReview(request.cwd);
      if (request.command === "archive") return await runArchive(request.cwd);
      const status = await runStatus(request.cwd);
      if (status.state === "NOT_INITIALIZED") {
        throw new SddError(
          "E_NOT_INITIALIZED",
          "Run sdd init before other commands",
          "sdd init",
        );
      }
      throw new SddError(
        "E_INVALID_PHASE_COMMAND",
        `Command ${request.command} is not implemented for ${status.state}`,
        status.next,
      );
    } catch (error) {
      if (!(error instanceof SddError)) throw error;
      const status = await runStatus(request.cwd).catch(() => ({
        ok: false,
        state: "FAILED" as const,
        exitCode: error.exitCode,
      }));
      return {
        ok: false,
        state: status.state,
        exitCode: error.exitCode,
        error: error.toCommandError(),
      };
    }
  }

  private async runAuto(request: CommandRequest): Promise<CommandResult> {
    let status = await runStatus(request.cwd);
    if (status.state === "NOT_INITIALIZED") {
      throw new SddError(
        "E_NOT_INITIALIZED",
        "Run sdd init before sdd auto",
        "sdd init",
      );
    }
    const commandByPhase = {
      INDEX_READY: "new",
      SPEC_READY: "design",
      DESIGN_READY: "plan",
      PLAN_READY: "build",
      BUILD_READY: "verify",
      VERIFY_READY: "review",
      REVIEW_READY: "archive",
    } as const;
    for (let step = 0; step < 8; step += 1) {
      if (status.state === "ARCHIVED" || status.state === "CLARIFYING")
        return status;
      const command =
        commandByPhase[status.state as keyof typeof commandByPhase];
      if (command === undefined) return status;
      const result = await this.execute({
        command,
        cwd: request.cwd,
        ...(command === "new" && request.args !== undefined
          ? { args: request.args }
          : {}),
        ...(request.signal === undefined ? {} : { signal: request.signal }),
      });
      if (
        !result.ok ||
        result.state === "CLARIFYING" ||
        result.state === "ARCHIVED"
      )
        return result;
      status = result;
    }
    throw new SddError(
      "E_STATE_CORRUPTED",
      "Auto workflow exceeded the maximum phase count",
    );
  }
}
