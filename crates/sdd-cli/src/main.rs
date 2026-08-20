//! sdd CLI 入口 — 参数解析、命令路由、输出格式化。
//!
//! 参数与输出契约对齐 早期 Node 实现：
//! - 全局参数：--json/--cwd/--change/--timeout/--non-interactive/--verbose
//! - 命令：init/status/new/design/plan/build/verify/review/archive/auto/codebase
//! - 进程退出码必须等于 CommandResult.exitCode

use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use sdd_core::contracts::{CommandRequest, CommandResult, HostAdapter};
use sdd_core::error::SddError;

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

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
    /// 超时秒数（锁等待与子进程执行超时）
    #[arg(long, global = true, value_parser = parse_timeout)]
    timeout: Option<f64>,
    /// 无人值守模式；遇到未回答的需求阻塞问题直接失败
    #[arg(long, global = true, default_value_t = false)]
    non_interactive: bool,
    /// 详细输出
    #[arg(long, global = true, default_value_t = false)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
    /// 初始化 .sdd/
    Init {
        /// 空项目目录结构策略
        #[arg(long = "structurePolicy", value_parser = ["free-design", "user-defined"])]
        structure_policy: Option<String>,
        /// 宿主适配器（仅供宿主 Agent 内部传入，终端隐藏）
        #[arg(long = "host-adapter", hide = true)]
        host_adapter: Option<String>,
    },
    /// 显示当前 SDD 状态
    Status {
        /// 显示 loop 状态摘要
        #[arg(long = "loop")]
        loop_status: bool,
    },
    /// 创建新变更（需求）
    New {
        /// 需求文本（可多个词）
        requirement: Vec<String>,
        /// 澄清答案 JSON，如 {"Q-GOAL":"答案"}
        #[arg(long)]
        #[arg(value_parser = parse_answers)]
        answers: Option<serde_json::Value>,
    },
    /// 修订已有变更并同步所有文档
    Change {
        /// 目标变更 ID
        change_id: String,
        /// 新需求文本（可多个词）
        requirement: Vec<String>,
        /// 澄清答案 JSON，如 {"Q-ACTOR":"授权用户"}
        #[arg(long, value_parser = parse_answers)]
        answers: Option<serde_json::Value>,
    },
    /// 生成设计制品
    Design,
    /// 生成实施计划
    Plan {
        /// 计划内依赖决策 JSON 数组
        #[arg(long, value_parser = parse_dependencies)]
        dependencies: Option<serde_json::Value>,
    },
    /// 构建（build next / build complete）
    Build {
        /// 子命令：next 或 complete
        sub: Option<String>,
        /// complete 时的任务 ID（如 TASK-001-RED）
        #[arg(long)]
        task: Option<String>,
        /// complete 时内联提交的 TaskExecutionResult JSON
        #[arg(long = "result-json")]
        result_json: Option<String>,
        /// complete 时从文件读取 TaskExecutionResult JSON
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
        /// 澄清答案 JSON，如 {"Q-ACTOR":"答案"}
        #[arg(long, value_parser = parse_answers)]
        answers: Option<serde_json::Value>,
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
        /// 事件条数（必须与 --events 一起使用）
        #[arg(long)]
        tail: Option<u64>,
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
    let init_adapter = match resolve_init_adapter(&cli) {
        Ok(adapter) => adapter,
        Err(error) => return render_error_and_exit(&error, cli.global.json, "FAILED"),
    };
    let cwd = match &cli.global.cwd {
        Some(cwd) => cwd.clone(),
        None => match std::env::current_dir() {
            Ok(cwd) => cwd.to_string_lossy().to_string(),
            Err(error) => {
                let error =
                    SddError::new("E_PATH_OUTSIDE_REPO", &format!("无法读取当前目录：{error}"));
                return render_error_and_exit(&error, cli.global.json, "FAILED");
            }
        },
    };
    let (command, args) = build_request(&cli, init_adapter.as_deref());
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
        Ok(result) => render_and_exit(&result, cli.global.json),
        Err(error) => {
            let state = sdd_core::commands::status::read_phase(&request.cwd)
                .unwrap_or_else(|_| "FAILED".to_string());
            render_error_and_exit(&error, cli.global.json, &state)
        }
    }
}

/// `sdd init` 默认生成 Codex 原生资产；其他宿主通过隐藏选项自行标记。
fn resolve_init_adapter(cli: &Cli) -> Result<Option<String>, SddError> {
    let Command::Init { host_adapter, .. } = &cli.command else {
        return Ok(None);
    };
    if let Some(agent) = host_adapter {
        return validate_adapter(agent).map(|adapter| Some(adapter.as_str().to_string()));
    }
    Ok(Some(HostAdapter::DEFAULT.as_str().to_string()))
}

fn validate_adapter(adapter: &str) -> Result<HostAdapter, SddError> {
    HostAdapter::parse(adapter)
        .ok_or_else(|| SddError::new("E_INVALID_PHASE_COMMAND", "宿主 Agent 仅支持 Codex 或 OMP"))
}

fn render_error_and_exit(error: &SddError, json: bool, state: &str) -> ExitCode {
    let result = CommandResult::from_error(state, error);
    if json {
        render_and_exit(&result, true)
    } else {
        eprintln!("{}", render_text_error(error));
        ExitCode::from(clamp_exit(error.exit_code))
    }
}

/// 渲染 CommandResult 并退出：--json 输出稳定 JSON，否则输出可读文本
fn render_and_exit(result: &CommandResult, json: bool) -> ExitCode {
    let exit = clamp_exit(result.exit_code);
    if json {
        match serde_json::to_string_pretty(result) {
            Ok(text) => println!("{text}"),
            Err(e) => eprintln!("序列化结果失败：{e}"),
        }
    } else {
        println!("{}", render_text(result));
    }
    ExitCode::from(exit)
}

/// 文本渲染：状态、下一步与错误信息（对齐 Node 版 outputText 语义）
fn render_text(result: &CommandResult) -> String {
    let mut lines = Vec::new();
    if let Some(action) = &result.action_required {
        lines.push(format!("任务：{}", action.task_id));
        lines.push(format!("Context Pack：{}", action.context_pack));
        lines.push(format!("结果传输：{}", action.result_transport));
        lines.push(format!("允许文件：{}", action.allowed_files.join("、")));
        if !action.verification.is_empty() {
            let commands: Vec<String> = action
                .verification
                .iter()
                .map(|v| {
                    if v.args.is_empty() {
                        v.command.clone()
                    } else {
                        format!("{} {}", v.command, v.args.join(" "))
                    }
                })
                .collect();
            lines.push(format!("验证命令：{}", commands.join("；")));
        }
        return lines.join("\n");
    }
    if let Some(error) = &result.error {
        lines.push(format!("错误（{}）：{}", error.code, error.message));
        if let Some(next) = &error.next {
            lines.push(format!("建议：{next}"));
        }
    } else if result.ok {
        lines.push(format!("状态：{}", result.state));
    } else {
        lines.push(format!("状态：{}（未完成）", result.state));
    }
    if let Some(change_id) = &result.change_id {
        lines.push(format!("变更：{change_id}"));
    }
    if let Some(next) = &result.next {
        lines.push(format!("下一步：{next}"));
    }
    if let Some(warnings) = &result.warnings {
        for warning in warnings {
            if let Some(message) = warning.get("message").and_then(|m| m.as_str()) {
                lines.push(format!("警告：{message}"));
            }
        }
    }
    if let Some(data) = &result.data {
        if let Ok(text) = serde_json::to_string(data) {
            if !text.is_empty() && text != "null" {
                if text.chars().count() > 512 {
                    // 长数据在文本模式下只给提示，避免倾倒整个状态对象；完整内容走 --json。
                    lines.push(format!(
                        "数据：<JSON 过长，已省略 {} 字符；使用 --json 查看完整内容>",
                        text.chars().count()
                    ));
                } else {
                    lines.push(format!("数据：{text}"));
                }
            }
        }
    }
    lines.join("\n")
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
    // 文本模式也带错误码，便于脚本识别 E_* 码。
    let base = format!("错误（{}）：{}", error.code, error.message);
    match &error.next {
        Some(next) => format!("{base}\n建议：{next}"),
        None => base,
    }
}

/// 把 CLI 参数结构转换为 Core 的 args JSON（对齐 Node 版 extraArgs 键名）
fn build_request(cli: &Cli, init_agent: Option<&str>) -> (&'static str, serde_json::Value) {
    let mut args = serde_json::Map::new();
    let g = &cli.global;
    if let Some(change) = &g.change {
        args.insert("changeId".into(), serde_json::json!(change));
    }
    if let Some(timeout) = g.timeout {
        args.insert("timeout".into(), serde_json::json!(timeout));
    }
    if g.non_interactive {
        args.insert("nonInteractive".into(), serde_json::json!(true));
    }
    if g.verbose {
        args.insert("verbose".into(), serde_json::json!(true));
    }

    let command: &'static str = match &cli.command {
        Command::Init {
            structure_policy,
            host_adapter: _,
        } => {
            if let Some(policy) = structure_policy {
                args.insert("structurePolicy".into(), serde_json::json!(policy));
            }
            if let Some(agent) = init_agent {
                args.insert("hostAdapter".into(), serde_json::json!(agent));
            }
            "init"
        }
        Command::Status { loop_status } => {
            if *loop_status {
                args.insert("loopStatus".into(), serde_json::json!(true));
            }
            "status"
        }
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
                args.insert("answers".into(), answers.clone());
            }
            "new"
        }
        Command::Change {
            change_id,
            requirement,
            answers,
        } => {
            args.insert("changeId".into(), serde_json::json!(change_id));
            if !requirement.is_empty() {
                args.insert(
                    "requirement".into(),
                    serde_json::json!(requirement.join(" ")),
                );
            }
            if let Some(answers) = answers {
                args.insert("answers".into(), answers.clone());
            }
            "change"
        }
        Command::Design => "design",
        Command::Plan { dependencies } => {
            if let Some(dependencies) = dependencies {
                args.insert("dependencies".into(), dependencies.clone());
            }
            "plan"
        }
        Command::Build {
            sub,
            task,
            result_json,
            result,
        } => {
            if let Some(sub) = sub {
                args.insert("sub".into(), serde_json::json!(sub));
            }
            if let Some(task) = task {
                args.insert("task".into(), serde_json::json!(task));
            }
            if let Some(result_json) = result_json {
                args.insert("resultJson".into(), serde_json::json!(result_json));
            }
            if let Some(result) = result {
                args.insert("resultPath".into(), serde_json::json!(result));
            }
            "build"
        }
        Command::Verify => "verify",
        Command::Review => "review",
        Command::Archive => "archive",
        Command::Auto {
            requirement,
            answers,
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
            if let Some(answers) = answers {
                args.insert("answers".into(), answers.clone());
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
                args.insert("tail".into(), serde_json::json!(tail));
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

fn parse_answers(raw: &str) -> Result<serde_json::Value, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|error| format!("answers 不是合法 JSON：{error}"))?;
    if !value.is_object() {
        return Err("answers 必须是 JSON 对象".to_string());
    }
    Ok(value)
}

fn parse_timeout(raw: &str) -> Result<f64, String> {
    let seconds = raw
        .parse::<f64>()
        .map_err(|_| "timeout 必须是非负数字".to_string())?;
    if !seconds.is_finite() || seconds < 0.0 {
        return Err("timeout 必须是非负有限数字".to_string());
    }
    Ok(seconds)
}

fn parse_dependencies(raw: &str) -> Result<serde_json::Value, String> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|error| format!("dependencies 不是合法 JSON：{error}"))?;
    if !value.is_array() {
        return Err("dependencies 必须是 JSON 数组".to_string());
    }
    Ok(value)
}
