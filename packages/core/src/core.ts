import { CodebaseAdapter } from "./codebase/codebase-adapter.js";
import { runInit } from "./commands/init.js";
import { runNew } from "./commands/new.js";
import { runDesign } from "./commands/design.js";
import { runPlan } from "./commands/plan.js";
import { runBuild } from "./commands/build.js";
import { runVerify } from "./commands/verify.js";
import { runReview } from "./commands/review.js";
import { runArchive } from "./commands/archive.js";
import { runCodebaseCommand } from "./commands/codebase.js";
import { LoopEngine } from "./loop/loop-engine.js";
import { LoopStore } from "./loop/loop-store.js";
import { LoopEventStore } from "./loop/loop-events.js";
import { StateStore } from "./state/state-store.js";
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

/**
 * Core 是整个工作流的统一调度入口。
 * 所有平台适配器最终都只通过这里推进状态机、写入制品和返回结果。
 */
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
      // status 是纯只读命令，不依赖完整的写命令分发流程。
      if (request.command === "status")
        return withVerboseData(await runStatus(request.cwd), request);
      // codebase 子命令：委托 CodebaseAdapter 处理
      if (request.command === "codebase")
        return withVerboseData(
          await runCodebaseCommand(request.cwd, this.codebase, request.args),
          request,
        );
      if (request.command === "init")
        return withVerboseData(
          await runInit(
            request.cwd,
            this.codebase,
            request.args,
            request.signal,
          ),
          request,
        );
      const current = await runStatus(request.cwd);
      // 已归档 change 进入只读模式，只允许重新 archive、查看状态或开启新 change。
      if (
        current.state === "ARCHIVED" &&
        request.command !== "archive" &&
        request.command !== "new"
      ) {
        throw new SddError("E_ARCHIVED_READONLY", "已归档的变更为只读状态");
      }
      if (request.command === "auto") return await this.runAuto(request);
      if (request.command === "new")
        return withVerboseData(
          await runNew(
            request.cwd,
            request.args,
            this.specEngine,
            this.codebase,
            request.signal,
          ),
          request,
        );
      if (request.command === "design")
        return withVerboseData(
          await runDesign(
            request.cwd,
            this.tddEngine,
            request.args,
            request.signal,
          ),
          request,
        );
      if (request.command === "plan")
        return withVerboseData(
          await runPlan(
            request.cwd,
            this.tddEngine,
            request.args,
            request.signal,
          ),
          request,
        );
      if (request.command === "build")
        return withVerboseData(
          await runBuild(
            request.cwd,
            this.taskExecutor,
            request.signal,
            request.args,
          ),
          request,
        );
      if (request.command === "verify")
        return withVerboseData(
          await runVerify(request.cwd, request.args, request.signal),
          request,
        );
      if (request.command === "review")
        return withVerboseData(
          await runReview(request.cwd, request.args, request.signal),
          request,
        );
      if (request.command === "archive")
        return withVerboseData(
          await runArchive(request.cwd, request.args, request.signal),
          request,
        );
      const status = await runStatus(request.cwd);
      if (status.state === "NOT_INITIALIZED") {
        throw new SddError(
          "E_NOT_INITIALIZED",
          "请先运行 sdd init 再执行其他命令",
          "sdd init",
        );
      }
      throw new SddError(
        "E_INVALID_PHASE_COMMAND",
        `命令 ${request.command} 在状态 ${status.state} 下不可用`,
        status.next,
      );
    } catch (error) {
      if (!(error instanceof SddError)) throw error;
      const status = await runStatus(request.cwd).catch(() => ({
        ok: false,
        state: "FAILED" as const,
        exitCode: error.exitCode,
      }));
      return withVerboseData(
        {
          ok: false,
          state: status.state,
          exitCode: error.exitCode,
          error: error.toCommandError(),
        },
        request,
      );
    }
  }

  private async runAuto(request: CommandRequest): Promise<CommandResult> {
    const loopEngine = new LoopEngine(
      request.cwd,
      new StateStore(request.cwd),
      new LoopStore(request.cwd),
      new LoopEventStore(request.cwd),
      (req) => this.execute(req),
    );
    return loopEngine.run(request);
  }
}

function withVerboseData(
  result: CommandResult,
  request: CommandRequest,
): CommandResult {
  if (request.args?.verbose !== true) return result;
  const debug = {
    command: request.command,
    cwd: request.cwd,
    verbose: true,
    ...(request.args === undefined ? {} : { args: request.args }),
  };
  if (
    result.data !== undefined &&
    typeof result.data === "object" &&
    result.data !== null &&
    !Array.isArray(result.data)
  ) {
    return {
      ...result,
      data: {
        ...result.data,
        debug,
      },
    };
  }
  return {
    ...result,
    data:
      result.data === undefined
        ? { debug }
        : {
            value: result.data,
            debug,
          },
  };
}
