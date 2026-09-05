//! sdd CLI 入口 — 参数解析、命令路由、输出格式化。
//!
//! 参数与输出遵循当前 Core 契约：
//! - 全局参数：--json/--cwd/--change/--timeout
//! - 命令：init/status/spec/change/plan/build/verify/archive/codebase
//! - 进程退出码必须等于 CommandResult.exitCode

use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use sdd_core::commands::status::phase_label;
use sdd_core::contracts::{CommandRequest, CommandResult, HostAdapter};
use sdd_core::error::SddError;

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "sdd", version = PKG_VERSION, about = "面向 AI Coding Agent 的规格驱动开发（SDD）工程支架", disable_help_subcommand = true)]
#[command(
    after_help = "快速开始：\n  在业务项目中运行 sdd init，再在 Codex 或 OMP 中描述要完成的需求。\n  例如：请实现订单导出，并完成测试和验证。\n  已有任务用 sdd status 查看进度，多个任务用 --change <标识> 选择。\n\n终端示例：sdd spec \"增加订单导出功能\"\nCLI 不会自行启动 AI；阶段结果由宿主 Agent 提交，用户无需手写 JSON。"
)]
struct Cli {
    /// 全局参数（任何命令前均可指定）
    #[command(flatten)]
    global: GlobalArgs,
    #[command(subcommand)]
    command: Command,
}

/// 全局参数。
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
}

#[derive(Subcommand)]
enum Command {
    /// 初始化项目并安装宿主 Agent 技能
    Init {
        /// 空项目目录结构策略
        #[arg(long = "structurePolicy", value_parser = ["free-design", "user-defined"])]
        structure_policy: Option<String>,
        /// 宿主适配器（仅供宿主 Agent 内部传入，终端隐藏）
        #[arg(long = "host-adapter", hide = true)]
        host_adapter: Option<String>,
    },
    /// 显示当前 SDD 状态
    Status,
    /// 创建统一规格（需求与技术设计）
    Spec {
        /// 需求文本（可多个词）
        requirement: Vec<String>,
        /// 宿主 Agent 回传的统一规格 JSON
        #[arg(long = "result-json")]
        result_json: Option<String>,
    },
    /// 修订已有变更并重新生成统一规格
    Change {
        /// 新需求文本（可多个词）
        requirement: Vec<String>,
        /// 宿主 Agent 回传的统一规格 JSON
        #[arg(long = "result-json")]
        result_json: Option<String>,
    },
    /// 生成实施计划
    Plan {
        /// 宿主 Agent 回传的结构化计划 JSON
        #[arg(long = "result-json")]
        result_json: Option<String>,
    },
    /// 构建（build next / build complete）
    Build {
        /// 子命令：next 或 complete
        #[arg(value_parser = ["next", "complete"])]
        sub: Option<String>,
        /// complete 时的任务 ID（如 TASK-001）
        #[arg(long)]
        task: Option<String>,
        /// complete 时内联提交的 TaskExecutionResult JSON
        #[arg(long = "result-json")]
        result_json: Option<String>,
    },
    /// 验证
    Verify {
        /// 用户明确授权质量阻断后的下一轮修复
        #[arg(long = "continue", default_value_t = false)]
        continue_fix: bool,
        /// 宿主 Agent 回传的结构化修复结果 JSON
        #[arg(long = "result-json")]
        result_json: Option<String>,
    },
    /// 归档
    Archive,
    /// 代码库上下文管理（status/doctor/index/query/rebuild）
    Codebase {
        /// 子命令：status/doctor/index/query/rebuild
        #[arg(value_parser = ["status", "doctor", "index", "query", "rebuild"])]
        sub: Option<String>,
        /// 查询词（query 子命令）
        query_parts: Vec<String>,
        /// 查询 intent（impact/context/explore/callers/callees 等）
        #[arg(long)]
        intent: Option<String>,
    },
}

fn main() -> ExitCode {
    // 无命令时显示帮助并退出 0；未知命令由 clap 退出 2。
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
            if let Err(error) = e.print() {
                eprintln!("输出命令行错误失败：{error}");
            }
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
            Ok(cwd) => match cwd.into_os_string().into_string() {
                Ok(cwd) => cwd,
                Err(_) => {
                    let error = SddError::new(
                        "E_PATH_OUTSIDE_REPO",
                        "当前目录不是有效 UTF-8，无法写入 JSON 契约；请使用 --cwd 指定可表示路径",
                    );
                    return render_error_and_exit(&error, cli.global.json, "FAILED");
                }
            },
            Err(error) => {
                let error =
                    SddError::new("E_PATH_OUTSIDE_REPO", &format!("无法读取当前目录：{error}"));
                return render_error_and_exit(&error, cli.global.json, "FAILED");
            }
        },
    };
    let (command, args) = match build_request(&cli, init_adapter.as_deref()) {
        Ok(request) => request,
        Err(error) => return render_error_and_exit(&error, cli.global.json, "FAILED"),
    };
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
        Err(error) => match sdd_core::commands::status::read_phase(&request.cwd) {
            Ok(state) => render_error_and_exit(&error, cli.global.json, &state),
            Err(state_error) => render_error_and_exit(&state_error, cli.global.json, "FAILED"),
        },
    }
}

/// `sdd init` 默认生成 Codex 原生资产；其他宿主通过隐藏选项自行标记。
fn resolve_init_adapter(cli: &Cli) -> Result<Option<String>, SddError> {
    let Command::Init { host_adapter, .. } = &cli.command else {
        return Ok(None);
    };
    if let Some(adapter) = host_adapter {
        return validate_adapter(adapter).map(|adapter| Some(adapter.as_str().to_string()));
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
            Err(e) => {
                eprintln!("序列化结果失败：{e}");
                return ExitCode::from(1);
            }
        }
    } else {
        println!("{}", render_text(result));
    }
    ExitCode::from(exit)
}

/// 文本渲染：状态、下一步与错误信息。
fn render_text(result: &CommandResult) -> String {
    let mut lines = Vec::new();
    if let Some(data) = &result.data {
        if render_codebase(&mut lines, data) {
            return lines.join("\n");
        }
    }
    if let Some(action) = &result.action_required {
        use sdd_core::contracts::AgentActionRequired;
        match action {
            AgentActionRequired::AgentPhaseExecution { phase, .. } => {
                let stage = if phase == "PLAN" {
                    "实施计划"
                } else {
                    "规格与技术设计"
                };
                lines.push(format!("等待 Agent 完成{stage}。"));
            }
            AgentActionRequired::AgentTaskExecution {
                task_id,
                allowed_files,
                verification,
                ..
            } => {
                lines.push(format!("等待 Agent 实施任务：{task_id}"));
                lines.push(format!("允许文件：{}", allowed_files.join("、")));
                render_verification(&mut lines, verification);
            }
            AgentActionRequired::AgentFixExecution {
                allowed_files,
                verification,
                ..
            } => {
                lines.push("质量检查需要修复，等待 Agent 处理报告中的问题。".to_string());
                if let Some(report) = result.data.as_ref().and_then(|data| data.get("report")) {
                    render_report(&mut lines, report);
                }
                lines.push(format!("允许文件：{}", allowed_files.join("、")));
                render_verification(&mut lines, verification);
            }
        }
        if let Some(change_id) = &result.change_id {
            lines.push(format!("变更：{change_id}"));
        }
        lines.push("请在 Codex 或 OMP 中继续此需求；终端命令不会自行启动 AI。宿主使用 --json 读取完整行动和结果格式。".to_string());
        return lines.join("\n");
    }
    if let Some(error) = &result.error {
        lines.push(format!("错误（{}）：{}", error.code, error.message));
        if let Some(next) = &error.next {
            lines.push(format!("建议：{next}"));
        }
    } else if result.ok {
        lines.push(format!("状态：{}", phase_label(&result.state)));
    } else {
        lines.push(format!("状态：{}（未完成）", phase_label(&result.state)));
    }
    if let Some(change_id) = &result.change_id {
        lines.push(format!("变更：{change_id}"));
    }
    if result.state == "QUALITY_BLOCKED" {
        lines.push(
            "自动修复轮次已用完。可手动修复后重新验证，或明确授权 Agent 再修一轮。".to_string(),
        );
        if let Some(id) = &result.change_id {
            lines.push(format!("重新验证：sdd verify --change {id}"));
        }
        if let Some(next) = &result.next {
            lines.push(format!("授权后继续修复：{next}"));
        }
    } else if let Some(next) = &result.next {
        lines.push(format!("下一步：{next}"));
    }
    if let Some(warnings) = &result.warnings {
        for warning in warnings {
            if warning.code == "W_ADAPTER_FILE" {
                lines.push(format!(
                    "已安装：{}",
                    warning.message.trim_start_matches("写入：")
                ));
            } else if warning.code == "W_EMPTY_PROJECT" {
                lines.push(format!("提示：{}", warning.message));
            } else {
                lines.push(format!("警告：{}", warning.message));
            }
        }
    }
    if let Some(data) = &result.data {
        if let Some(report) = data.get("report").filter(|report| report.is_object()) {
            render_report(&mut lines, report);
            if data.get("activeChanges").is_none() {
                return lines.join("\n");
            }
        }
        if let Some(goal) = data.get("goal").and_then(serde_json::Value::as_str) {
            lines.push(format!("目标：{goal}"));
            return lines.join("\n");
        }
        if let Some(count) = data.get("taskCount").and_then(serde_json::Value::as_u64) {
            lines.push(format!("计划任务：{count} 个"));
            return lines.join("\n");
        }
        if let Some(task_id) = data.get("taskId").and_then(serde_json::Value::as_str) {
            lines.push(format!(
                "任务：{task_id}，{}",
                if data["status"] == "DONE" {
                    "已完成"
                } else {
                    "未完成"
                }
            ));
            return lines.join("\n");
        }
        if let Some(changes) = data
            .get("activeChanges")
            .and_then(serde_json::Value::as_array)
        {
            let selected = data.get("selectedChange");
            let visible = selected
                .map(std::slice::from_ref)
                .unwrap_or(changes.as_slice());
            for change in visible {
                let id = change["changeId"].as_str().unwrap_or_default();
                if result
                    .change_id
                    .as_deref()
                    .is_some_and(|selected| selected != id)
                {
                    continue;
                }
                lines.push(format!(
                    "- {} [{}]：{}",
                    change["title"].as_str().unwrap_or(id),
                    id,
                    phase_label(change["phase"].as_str().unwrap_or_default())
                ));
            }
            if result.state == "MULTIPLE_CHANGES" {
                lines.push("请选择变更，使用 --change <标识> 继续。".to_string());
            }
            if let Some(tasks) = data
                .pointer("/workflow/tasks")
                .and_then(serde_json::Value::as_object)
            {
                if !tasks.is_empty() {
                    let done = tasks
                        .values()
                        .filter(|state| state.as_str() == Some("DONE"))
                        .count();
                    lines.push(format!("任务进度：{done}/{} 已完成", tasks.len()));
                }
            }
            return lines.join("\n");
        }
        let text = serde_json::to_string(data).expect("serde_json::Value 必须可序列化");
        if !text.is_empty() && text != "null" {
            let char_count = text.chars().count();
            if char_count > 512 {
                // 长数据在文本模式下只给提示，避免倾倒整个状态对象；完整内容走 --json。
                lines.push(format!(
                    "数据：<JSON 过长，已省略 {} 字符；使用 --json 查看完整内容>",
                    char_count
                ));
            } else {
                lines.push(format!("数据：{text}"));
            }
        }
    }
    lines.join("\n")
}

fn render_report(lines: &mut Vec<String>, report: &serde_json::Value) {
    if let Some(summary) = report.get("summary").and_then(serde_json::Value::as_str) {
        lines.push(summary.to_string());
    }
    if let Some(issues) = report.get("issues").and_then(serde_json::Value::as_array) {
        for issue in issues {
            if let Some(message) = issue.get("message").and_then(serde_json::Value::as_str) {
                let file = issue
                    .get("file")
                    .and_then(serde_json::Value::as_str)
                    .map(|file| format!("（{file}）"))
                    .unwrap_or_default();
                lines.push(format!("- {message}{file}"));
            }
        }
    }
}

fn render_codebase(lines: &mut Vec<String>, data: &serde_json::Value) -> bool {
    if let Some(providers) = data.get("providers").and_then(serde_json::Value::as_array) {
        for provider in providers {
            let name = provider["provider"].as_str().unwrap_or("CodeGraph");
            let state = if provider["installed"] != true {
                "不可用"
            } else if provider["indexed"] != true {
                "索引不可用"
            } else {
                "索引已就绪"
            };
            lines.push(format!("{name}：{state}"));
            if let Some(reason) = provider["reason"].as_str() {
                lines.push(format!("原因：{reason}"));
            }
            if provider["degraded"] == true {
                lines.push("可继续开发，代码库上下文将使用受限文件扫描；索引可用后运行 sdd codebase index 更新。".to_string());
            }
        }
        return true;
    }
    if let Some(payload) = data
        .get("payload")
        .filter(|_| data.get("provider").is_some())
    {
        lines.push(format!(
            "代码库查询（{}）",
            data["provider"].as_str().unwrap_or_default()
        ));
        if let Some(reason) = data.get("reason").and_then(serde_json::Value::as_str) {
            lines.push(format!("提示：{reason}"));
        }
        for key in [
            "output",
            "codebaseSummary",
            "packageStructure",
            "architecture",
        ] {
            if let Some(text) = payload.get(key).and_then(serde_json::Value::as_str) {
                lines.push(text.to_string());
            }
        }
        return true;
    }
    false
}

fn render_verification(
    lines: &mut Vec<String>,
    verification: &[sdd_core::contracts::VerificationCommand],
) {
    if verification.is_empty() {
        return;
    }
    let commands = verification
        .iter()
        .map(|item| {
            std::iter::once(item.command.as_str())
                .chain(item.args.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>();
    lines.push(format!("验证命令：{}", commands.join("；")));
}

/// 将内部退出码约束到进程支持的 0..=255。
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

/// 把 CLI 参数结构转换为 Core 的 args JSON。
fn build_request(
    cli: &Cli,
    init_adapter: Option<&str>,
) -> Result<(&'static str, serde_json::Value), SddError> {
    let mut args = serde_json::Map::new();
    let g = &cli.global;
    if let Some(change) = &g.change {
        args.insert("changeId".into(), serde_json::json!(change));
    }
    if let Some(timeout) = g.timeout {
        args.insert("timeout".into(), serde_json::json!(timeout));
    }
    let command: &'static str = match &cli.command {
        Command::Init {
            structure_policy,
            host_adapter: _,
        } => {
            if let Some(policy) = structure_policy {
                args.insert("structurePolicy".into(), serde_json::json!(policy));
            }
            if let Some(adapter) = init_adapter {
                args.insert("hostAdapter".into(), serde_json::json!(adapter));
            }
            "init"
        }
        Command::Status => "status",
        Command::Spec {
            requirement,
            result_json,
        } => {
            if !requirement.is_empty() {
                args.insert(
                    "requirement".into(),
                    serde_json::json!(requirement.join(" ")),
                );
            }
            if let Some(result_json) = result_json {
                args.insert("resultJson".into(), serde_json::json!(result_json));
            }
            "spec"
        }
        Command::Change {
            requirement,
            result_json,
        } => {
            if !requirement.is_empty() {
                args.insert(
                    "requirement".into(),
                    serde_json::json!(requirement.join(" ")),
                );
            }
            if let Some(result_json) = result_json {
                args.insert("resultJson".into(), serde_json::json!(result_json));
            }
            "change"
        }
        Command::Plan { result_json } => {
            if let Some(result_json) = result_json {
                args.insert("resultJson".into(), serde_json::json!(result_json));
            }
            "plan"
        }
        Command::Build {
            sub,
            task,
            result_json,
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
            "build"
        }
        Command::Verify {
            continue_fix,
            result_json,
        } => {
            if *continue_fix {
                args.insert("continue".into(), serde_json::json!(true));
            }
            if let Some(result_json) = result_json {
                args.insert("resultJson".into(), serde_json::json!(result_json));
            }
            "verify"
        }
        Command::Archive => "archive",
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
    Ok((command, serde_json::Value::Object(args)))
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
