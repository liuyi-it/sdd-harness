//! sdd-core：SDD 状态机与质量门禁执行层。
//!
//! 翻译自 早期 Node 实现 的调度语义：
//! Core 是整个工作流的统一调度入口，所有平台适配器最终都只通过这里
//! 推进状态机、写入制品和返回结果。

pub mod assets;
pub mod commands;
pub mod contracts;
pub mod engines;
pub mod error;
pub mod git;
pub mod knowledge;
pub mod policies;
pub mod protocol;
pub mod quality;
pub mod schema;
pub mod security;
pub mod state;

use commands::status::{next_command, read_phase};
use contracts::{CommandRequest, CommandResult};
use error::SddError;

/// 统一调度入口：解析命令并分发到对应实现。
pub fn run(request: &CommandRequest) -> Result<CommandResult, SddError> {
    let cwd = &request.cwd;
    match request.command.as_str() {
        // status 是纯只读命令，不依赖完整分发流程
        "status" => commands::status::run_status(cwd, request.args.as_ref()),
        "codebase" => commands::codebase::run_codebase(cwd, request.args.as_ref()),
        "change" => {
            ensure_phase(cwd, request.command.as_str(), request.args.as_ref())?;
            commands::change::run_change(
                cwd,
                request.args.as_ref(),
                &engines::spec::spec_engine::SpecEngine::new(),
            )
        }
        "init" => commands::init::run_init(cwd, request.args.as_ref()),
        // 写命令统一先检查初始化状态
        "new" => {
            let phase = read_phase(cwd)?;
            if phase == "NOT_INITIALIZED" {
                return Err(
                    SddError::new("E_NOT_INITIALIZED", "请先运行 sdd init 再执行其他命令")
                        .with_next("sdd init"),
                );
            }
            commands::new::run_new(
                cwd,
                request.args.as_ref(),
                &engines::spec::spec_engine::SpecEngine::new(),
            )
        }
        "design" => {
            ensure_phase(cwd, request.command.as_str(), request.args.as_ref())?;
            commands::design::run_design(
                cwd,
                request.args.as_ref(),
                &engines::tdd::TddEngine::new(),
            )
        }
        "plan" => {
            ensure_phase(cwd, request.command.as_str(), request.args.as_ref())?;
            commands::plan::run_plan(cwd, request.args.as_ref(), &engines::tdd::TddEngine::new())
        }
        "build" => {
            ensure_phase(cwd, request.command.as_str(), request.args.as_ref())?;
            commands::build::run_build(cwd, request.args.as_ref())
        }
        "verify" => {
            ensure_phase(cwd, request.command.as_str(), request.args.as_ref())?;
            commands::verify::run_verify(cwd, request.args.as_ref())
        }
        "review" => {
            ensure_phase(cwd, request.command.as_str(), request.args.as_ref())?;
            commands::review::run_review(cwd, request.args.as_ref())
        }
        "archive" => {
            ensure_phase(cwd, request.command.as_str(), request.args.as_ref())?;
            commands::archive::run_archive(cwd, request.args.as_ref())
        }
        "auto" => {
            ensure_phase(cwd, request.command.as_str(), request.args.as_ref())?;
            commands::auto::run_auto(cwd, request.args.as_ref())
        }
        _ => Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("命令 {} 不可用", request.command),
        )),
    }
}

/// 写命令前置检查：未初始化 / 已归档只读 / 阶段门禁（对齐 Node 版 core.ts 的分发前检查）
fn ensure_phase(
    cwd: &str,
    command: &str,
    args: Option<&serde_json::Value>,
) -> Result<(), SddError> {
    let state = state::StateStore::new(cwd.to_string()).read()?;
    let phase = state.current_phase.clone();
    if phase == "NOT_INITIALIZED" {
        return Err(
            SddError::new("E_NOT_INITIALIZED", "请先运行 sdd init 再执行其他命令")
                .with_next("sdd init"),
        );
    }
    if phase == "ARCHIVED" && command != "archive" && command != "new" && command != "auto" {
        return Err(SddError::new(
            "E_ARCHIVED_READONLY",
            "已归档的变更为只读状态",
        ));
    }

    if phase == "NEW_STARTED"
        && command != "new"
        && (state.current_change_id.is_none() || state.current_run_id.is_none())
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "NEW_STARTED 状态缺少可恢复的当前 change/run",
        )
        .with_next("sdd status"));
    }
    if let Some(requested) = args
        .and_then(|value| value.get("changeId"))
        .and_then(|value| value.as_str())
    {
        git::isolation::validate_change_id(requested)?;
        let active = state.current_change_id.as_deref().ok_or_else(|| {
            SddError::new("E_MISSING_CHANGE", "当前没有活动变更").with_next("sdd new")
        })?;
        if requested != active {
            return Err(SddError::new(
                "E_MISSING_CHANGE",
                &format!("指定变更 {requested} 不是当前活动变更 {active}"),
            ));
        }
    }
    // 阶段门禁：命令在特定阶段才可用（与 Node 版状态机语义一致）
    let allowed = match command {
        "change" => matches!(
            phase.as_str(),
            "SPEC_READY"
                | "DESIGN_READY"
                | "PLAN_READY"
                | "BUILD_WAITING_AGENT"
                | "BUILD_READY"
                | "VERIFY_READY"
                | "REVIEW_READY"
        ),
        "design" => phase == "SPEC_READY" || phase == "DESIGN_READY",
        "plan" => phase == "DESIGN_READY" || phase == "PLAN_READY",
        "build" => {
            phase == "PLAN_READY" || phase == "BUILD_WAITING_AGENT" || phase == "BUILD_READY"
        }
        "verify" => phase == "BUILD_READY",
        "review" => phase == "VERIFY_READY",
        "archive" => phase == "REVIEW_READY" || phase == "ARCHIVED",
        _ => true,
    };
    if !allowed {
        let next = next_command(&phase).unwrap_or_else(|| "sdd status".to_string());
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("命令 {command} 在状态 {phase} 下不可用"),
        )
        .with_next(&next));
    }
    Ok(())
}

/// 把错误结果转为 CommandResult（供 CLI 统一渲染）
pub fn result_from_error(state: &str, error: &SddError) -> CommandResult {
    CommandResult::from_error(state, error)
}
