//! sdd / sdd-harness CLI 入口 — 参数解析、命令路由、输出格式化。
//!
//! 参数与输出契约对齐 Node 版 `packages/cli/src/cli.ts`：
//! - 全局参数：--json/--cwd/--change/--timeout/--non-interactive/--force/--verbose
//! - 命令：init/status/new/design/plan/build/verify/review/archive/auto/codebase
//! - 进程退出码必须等于 CommandResult.exitCode

use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use sdd_core::contracts::{CommandRequest, CommandResult};
use sdd_core::error::SddError;

const PKG_VERSION: &str = "0.2.0";

#[derive(Parser)]
#[command(name = "sdd", version = PKG_VERSION, about = "面向 AI Coding Agent 的规格驱动开发（SDD）工程支架", disable_help_subcommand = true)]
struct Cli {
    /// 全局参数（任何命令前均可指定）
    #[command(flatten)]
    global: GlobalArgs,
    #[command(subcommand)]
    command: Command,
}

/// 全局参数（对齐 Node 版 parseArgs 的通用 options）
#[derive(Args)]
struct GlobalArgs {
    /// JSON 输出
    #[arg(long, global = true, default_value_t = false)]
    json: bool,
    /// 项目根目录（默认当前目录）
    #[arg(long, global = true)]
    cwd: Option<String>,
    /// 指定变更 ID
    #[arg(long, global = true)]
    change: Option<String>,
    /// 超时秒数
    #[arg(long, global = true)]
    timeout: Option<String>,
    /// 无人值守模式；遇到未回答的需求阻塞问题直接失败
    #[arg(long, global = true, default_value_t = false)]
    non_interactive: bool,
    /// 强制
    #[arg(long, global = true, default_value_t = false)]
    force: bool,
    /// 详细输出
    #[arg(long, global = true, default_value_t = false)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
    /// 初始化 .sdd/
    Init {
        /// 指定 Agent（claude/codex/opencode），可逗号分隔
        #[arg(long)]
        agent: Option<String>,
        /// 空项目目录结构策略
        #[arg(long)]
        structure_policy: Option<String>,
    },
    /// 显示当前 SDD 状态
    Status {
        /// 显示 loop 状态摘要
        #[arg(long)]
        loop_status: bool,
    },
    /// 创建新变更（需求）
    New {
        /// 需求文本（可多个词）
        requirement: Vec<String>,
        /// 澄清答案 JSON，如 {"Q-001":"答案"}
        #[arg(long)]
        answers: Option<String>,
    },
    /// 生成设计制品
    Design,
    /// 生成实施计划
    Plan,
    /// 构建（build next / build complete）
    Build {
        /// 子命令：next 或 complete
        sub: Option<String>,
        /// complete 时的任务 ID（如 TASK-001-RED）
        #[arg(long)]
        task: Option<String>,
        /// complete 时的任务结果文件路径
        #[arg(long)]
        result: Option<String>,
    },
    /// 验证
    Verify,
    /// 审查
    Review,
    /// 归档
    Archive,
    /// 自动推进 SDD Loop
    Auto {
        /// 需求文本（可多个词）
        requirement: Vec<String>,
        /// 恢复当前 auto run
        #[arg(long, default_value_t = false)]
        resume: bool,
        /// 重启 auto run
        #[arg(long, default_value_t = false)]
        restart: bool,
        /// 停止当前 auto run
        #[arg(long, default_value_t = false)]
        stop: bool,
        /// 查看 auto run 事件
        #[arg(long, default_value_t = false)]
        events: bool,
        /// 事件条数（与 --events 配合）
        #[arg(long)]
        tail: Option<String>,
        /// 查看 auto loop 状态
        #[arg(long, default_value_t = false)]
        loop_status: bool,
        /// 指定 run id（与 --resume 配合）
        #[arg(long)]
        run: Option<String>,
    },
    /// 代码库上下文管理（status/doctor/index/query/rebuild）
    Codebase {
        /// 子命令：status/doctor/index/query/rebuild
        sub: Option<String>,
        /// 查询词（query 子命令）
        query_parts: Vec<String>,
        /// 查询 intent（impact/context/explore/callers/callees 等）
        #[arg(long)]
        intent: Option<String>,
    },
}

fn main() -> ExitCode {
    // 无命令时显示帮助并退出 0（对齐 Node 版行为）；未知命令由 clap 退出 2
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e)
            if e.kind() == clap::error::ErrorKind::DisplayHelp
                || e.kind() == clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand =>
        {
            print!("{e}");
            return ExitCode::from(0);
        }
        Err(e) if e.kind() == clap::error::ErrorKind::DisplayVersion => {
            print!("{e}");
            return ExitCode::from(0);
        }
        Err(e) => {
            e.print().ok();
            return ExitCode::from(2);
        }
    };
    let (command, args) = build_request(&cli);
    let cwd = cli.global.cwd.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string()
    });
    let args = if args.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        None
    } else {
        Some(args)
    };
    let request = CommandRequest {
        command: command.to_string(),
        cwd,
        args,
    };
    match sdd_core::run(&request) {
        Ok(result) => render_and_exit(result),
        Err(error) => {
            // Core 错误统一转为 CommandResult 再渲染（与 Node 版 CLI 一致：
            // process.exit(result.exitCode)，错误信息进 stderr）
            eprintln!("{}", render_text_error(&error));
            ExitCode::from(clamp_exit(error.exit_code))
        }
    }
}

fn render_and_exit(result: CommandResult) -> ExitCode {
    let exit = clamp_exit(result.exit_code);
    let _ = result;
    ExitCode::from(exit)
}

/// 退出码截断到 0..=255（与 Node process.exitCode 行为一致）
fn clamp_exit(code: i32) -> u8 {
    if code < 0 {
        1
    } else if code > 255 {
        255
    } else {
        code as u8
    }
}

fn render_text_error(error: &SddError) -> String {
    match &error.next {
        Some(next) => format!("{}\n建议：{}", error.message, next),
        None => error.message.clone(),
    }
}

/// 把 CLI 参数结构转换为 Core 的 args JSON（对齐 Node 版 extraArgs 键名）
fn build_request(cli: &Cli) -> (&'static str, serde_json::Value) {
    let mut args = serde_json::Map::new();
    let g = &cli.global;
    if let Some(change) = &g.change {
        args.insert("changeId".into(), serde_json::json!(change));
    }
    if let Some(timeout) = &g.timeout {
        if let Ok(n) = timeout.parse::<f64>() {
            args.insert("timeout".into(), serde_json::json!(n));
        }
    }
    if g.non_interactive {
        args.insert("nonInteractive".into(), serde_json::json!(true));
    }
    if g.force {
        args.insert("force".into(), serde_json::json!(true));
    }
    if g.verbose {
        args.insert("verbose".into(), serde_json::json!(true));
    }

    let command: &'static str = match &cli.command {
        Command::Init {
            agent,
            structure_policy,
        } => {
            if let Some(agent) = agent {
                args.insert("agent".into(), serde_json::json!(agent));
            }
            if let Some(policy) = structure_policy {
                args.insert("structurePolicy".into(), serde_json::json!(policy));
            }
            "init"
        }
        Command::Status { .. } => "status",
        Command::New {
            requirement,
            answers,
        } => {
            if !requirement.is_empty() {
                args.insert(
                    "requirement".into(),
                    serde_json::json!(requirement.join(" ")),
                );
            }
            if let Some(answers) = answers {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(answers) {
                    args.insert("answers".into(), parsed);
                }
            }
            "new"
        }
        Command::Design => "design",
        Command::Plan => "plan",
        Command::Build { sub, task, result } => {
            if let Some(sub) = sub {
                args.insert("sub".into(), serde_json::json!(sub));
            }
            if let Some(task) = task {
                args.insert("task".into(), serde_json::json!(task));
            }
            if let Some(result) = result {
                args.insert("result".into(), serde_json::json!(result));
            }
            "build"
        }
        Command::Verify => "verify",
        Command::Review => "review",
        Command::Archive => "archive",
        Command::Auto {
            requirement,
            resume,
            restart,
            stop,
            events,
            tail,
            loop_status,
            run,
        } => {
            if !requirement.is_empty() {
                args.insert(
                    "requirement".into(),
                    serde_json::json!(requirement.join(" ")),
                );
            }
            if *resume {
                args.insert("resume".into(), serde_json::json!(true));
            }
            if let Some(run) = run {
                args.insert("run".into(), serde_json::json!(run));
            }
            if *restart {
                args.insert("restart".into(), serde_json::json!(true));
            }
            if *stop {
                args.insert("stop".into(), serde_json::json!(true));
            }
            if *events {
                args.insert("events".into(), serde_json::json!(true));
            }
            if let Some(tail) = tail {
                if let Ok(n) = tail.parse::<f64>() {
                    args.insert("tail".into(), serde_json::json!(n));
                }
            }
            if *loop_status {
                args.insert("loopStatus".into(), serde_json::json!(true));
            }
            "auto"
        }
        Command::Codebase {
            sub,
            query_parts,
            intent,
        } => {
            if let Some(sub) = sub {
                args.insert("sub".into(), serde_json::json!(sub));
            }
            if !query_parts.is_empty() {
                args.insert("query".into(), serde_json::json!(query_parts.join(" ")));
            }
            if let Some(intent) = intent {
                args.insert("intent".into(), serde_json::json!(intent));
            }
            "codebase"
        }
    };
    (command, serde_json::Value::Object(args))
}
