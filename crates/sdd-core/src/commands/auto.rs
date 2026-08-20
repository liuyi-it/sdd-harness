//! auto 命令：自动推进 SDD Loop。
//!
//! 翻译自 早期 Node 实现 + loop-engine 的核心语义：
//! - 确定性步骤（new→design→plan→build→verify→review→archive）自动推进
//! - 遇到澄清（CLARIFYING）或 Agent 编码（BUILD_WAITING_AGENT）时暂停，
//!   返回当前状态与原因，不绕过交互边界
//! - 失败预算：单步失败即暂停（stopOnFailure）

use serde_json::json;

use crate::contracts::CommandResult;

use crate::engines::spec::spec_engine::SpecEngine;
use crate::engines::tdd::TddEngine;
use crate::error::SddError;
use crate::state::StateStore;

pub fn run_auto(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    let empty_args = serde_json::Value::Null;
    let args = args.unwrap_or(&empty_args);
    // --tail 只对事件查看有意义，必须与 --events 一起使用。
    if args.get("tail").is_some()
        && !args
            .get("events")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    {
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            "--tail 必须与 --events 一起使用",
        ));
    }
    let timeout_ms = args
        .get("timeout")
        .and_then(|value| value.as_f64())
        .map(|seconds| (seconds * 1000.0) as u64);
    let _auto_guard = crate::state::file_lock::lock_auto(
        cwd,
        "sdd auto",
        args.get("changeId").and_then(|value| value.as_str()),
        timeout_ms,
    )?;
    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    let resume = args
        .get("resume")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let restart = args
        .get("restart")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if resume && restart {
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            "--resume 与 --restart 不能同时使用",
        ));
    }
    if args
        .get("events")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return read_events(cwd, &state, args);
    }
    if args
        .get("loopStatus")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Ok(loop_status_result(&state));
    }

    if args.get("stop").and_then(|v| v.as_bool()).unwrap_or(false) {
        let context = active_loop(&state)?;
        update_loop(&store, &context, "ABORTED", None)?;
        // 停止只终止 loop，不改变工作流阶段；恢复时应回到原有 Agent/命令边界。
        {
            let _guard = crate::state::file_lock::lock_sdd(
                cwd,
                "sdd auto stop",
                state.current_change_id.as_deref(),
                timeout_ms.or(Some(5_000)),
            )?;
            store.update(|s| {
                s.in_progress_phase = None;
                s.suggested_command = Some("sdd auto --resume".to_string());
            })?;
        }
        append_event(cwd, &context, "LOOP_STOPPED", &state.current_phase, None)?;
        return Ok(paused_result_with_reason(
            &state.current_phase,
            "auto loop 已停止；工作流阶段未改变",
        ));
    }

    let context = prepare_loop(&store, &state, resume, restart, args)?;
    append_event(
        cwd,
        &context,
        if resume {
            "LOOP_RESUMED"
        } else {
            "LOOP_STARTED"
        },
        &state.current_phase,
        None,
    )?;

    let requirement = args
        .get("requirement")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let spec_engine = SpecEngine::new();

    // 第一步：new。INDEX_READY 必须提供需求，其它可恢复阶段从 runtime 读取原始需求。
    // ARCHIVED 且提供了新需求时也要走 new 开启新变更；无需求保持走 archive 幂等分支。
    let mut phase = state.current_phase.clone();
    if matches!(
        phase.as_str(),
        "INDEX_READY" | "CLARIFYING" | "FAILED" | "PAUSED" | "NEW_STARTED"
    ) || (phase == "ARCHIVED" && requirement.is_some())
    {
        if phase == "INDEX_READY" && requirement.is_none() {
            update_loop(&store, &context, "PAUSED", Some("CLARIFICATION"))?;
            append_event(cwd, &context, "LOOP_PAUSED", &phase, None)?;
            return Ok(paused_result_with_reason(
                &phase,
                "auto 需要需求文本（sdd auto \"<需求>\"）",
            ));
        }
        let mut new_args = serde_json::Map::new();
        if let Some(requirement) = requirement.clone() {
            new_args.insert("requirement".to_string(), json!(requirement));
        }
        for key in ["changeId", "nonInteractive", "timeout", "answers"] {
            if let Some(value) = args.get(key) {
                new_args.insert(key.to_string(), value.clone());
            }
        }
        let new_result = crate::commands::new::run_new(
            cwd,
            Some(&serde_json::Value::Object(new_args)),
            &spec_engine,
        )?;
        if new_result.state == "CLARIFYING" {
            update_loop(&store, &context, "PAUSED", Some("CLARIFICATION"))?;
            append_event(cwd, &context, "LOOP_PAUSED", "CLARIFYING", Some("new"))?;
            return Ok(paused_result_with_reason(
                "CLARIFYING",
                "需求存在未回答的阻塞问题，请用 sdd auto --resume --answers '<JSON>' 继续",
            ));
        }
        phase = new_result.state.clone();
    }

    // 确定性步骤链：design → plan → build next →（Agent 完成全部任务后）verify → review → archive
    type StepFn = fn(&str, Option<&serde_json::Value>) -> Result<CommandResult, SddError>;
    let steps: [(&str, StepFn); 6] = [
        ("design", |cwd, args| {
            crate::commands::design::run_design(cwd, args, &TddEngine::new())
        }),
        ("plan", |cwd, args| {
            crate::commands::plan::run_plan(cwd, args, &TddEngine::new())
        }),
        ("build", |cwd, args| {
            crate::commands::build::run_build(cwd, args)
        }),
        ("verify", |cwd, args| {
            crate::commands::verify::run_verify(cwd, args)
        }),
        ("review", |cwd, args| {
            crate::commands::review::run_review(cwd, args)
        }),
        ("archive", |cwd, args| {
            crate::commands::archive::run_archive(cwd, args)
        }),
    ];

    for (name, step) in steps {
        let current = crate::commands::status::read_phase(cwd)?;
        let should_run = match name {
            "design" => current == "SPEC_READY",
            "plan" => current == "DESIGN_READY",
            // build 只在等待/可派发任务时运行；BUILD_READY（任务已全部完成）跳过
            "build" => current == "PLAN_READY" || current == "BUILD_WAITING_AGENT",
            "verify" => current == "BUILD_READY",
            "review" => current == "VERIFY_READY",
            "archive" => current == "REVIEW_READY" || current == "ARCHIVED",
            _ => false,
        };
        if !should_run {
            continue;
        }
        match step(cwd, Some(args)) {
            Ok(result) => {
                append_event(cwd, &context, "COMMAND_FINISHED", &result.state, Some(name))?;
                if result.state == "BUILD_WAITING_AGENT" {
                    // Agent 编码边界：暂停，返回 actionRequired
                    update_loop(
                        &store,
                        &context,
                        "WAITING_AGENT",
                        Some("AGENT_TASK_EXECUTION"),
                    )?;
                    append_event(cwd, &context, "ACTION_REQUIRED", &result.state, Some(name))?;
                    return Ok(result);
                }
                phase = result.state.clone();
                if result.state == "ARCHIVED" {
                    update_loop(&store, &context, "SUCCEEDED", None)?;
                    append_event(cwd, &context, "LOOP_ARCHIVED", "ARCHIVED", Some(name))?;
                    return Ok(CommandResult {
                        ok: true,
                        state: "ARCHIVED".to_string(),
                        exit_code: 0,
                        change_id: result.change_id.clone(),
                        next: Some("sdd new <需求>".to_string()),
                        data: Some(json!({ "loop": "COMPLETED" })),
                        rendered: None,
                        warnings: None,
                        action_required: None,
                        error: None,
                    });
                }
            }
            Err(e) => {
                {
                    let _guard = crate::state::file_lock::lock_sdd(
                        cwd,
                        "sdd auto failure",
                        state.current_change_id.as_deref(),
                        timeout_ms.or(Some(5_000)),
                    )?;
                    // 恢复语义落地：步骤失败把工作流阶段置为 PAUSED 并清空 in_progress_phase，
                    // 与返回结果一致，使 sdd status 能给出 auto --resume 建议。
                    store.update(|s| {
                        s.current_phase = "PAUSED".to_string();
                        s.in_progress_phase = None;
                        s.failed_command = Some(format!("sdd {name}"));
                        s.failed_reason = Some(e.message.clone());
                        s.suggested_command = Some("sdd auto --resume".to_string());
                    })?;
                }
                update_loop(&store, &context, "FAILED", None)?;
                append_event(cwd, &context, "LOOP_FAILED", &phase, Some(name))?;
                return Ok(CommandResult {
                    ok: false,
                    state: "PAUSED".to_string(),
                    exit_code: e.exit_code,
                    change_id: None,
                    next: Some("sdd auto --resume".to_string()),
                    data: Some(json!({
                        "loop": "PAUSED",
                        "failedCommand": format!("sdd {name}"),
                        "reason": e.message,
                    })),
                    rendered: None,
                    warnings: None,
                    action_required: None,
                    error: Some(e.to_command_error()),
                });
            }
        }
    }

    update_loop(&store, &context, "PAUSED", None)?;
    append_event(cwd, &context, "LOOP_PAUSED", &phase, None)?;
    Ok(paused_result(&phase))
}

fn paused_result(phase: &str) -> CommandResult {
    paused_result_with_reason(phase, "auto 在当前阶段暂停")
}

fn paused_result_with_reason(phase: &str, reason: &str) -> CommandResult {
    CommandResult {
        ok: false,
        state: phase.to_string(),
        exit_code: 0,
        change_id: None,
        next: Some("sdd auto --resume".to_string()),
        data: Some(json!({ "loop": "PAUSED", "reason": reason })),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    }
}

#[derive(Clone)]
struct LoopContext {
    loop_id: String,
    run_id: String,
}

fn prepare_loop(
    store: &StateStore,
    state: &crate::state::WorkflowState,
    resume: bool,
    restart: bool,
    args: &serde_json::Value,
) -> Result<LoopContext, SddError> {
    if resume {
        let context = active_loop(state)?;
        if let Some(run_id) = args.get("run").and_then(|value| value.as_str()) {
            crate::state::state_store::validate_run_id(run_id)?;
            if run_id != context.run_id {
                return Err(SddError::new(
                    "E_INVALID_PHASE_COMMAND",
                    "只能恢复当前 activeLoop；历史 run 请使用 --restart",
                ));
            }
        }
        update_loop(store, &context, "RUNNING", None)?;
        return Ok(context);
    }
    if !restart {
        if let Ok(context) = active_loop(state) {
            update_loop(store, &context, "RUNNING", None)?;
            return Ok(context);
        }
    }
    let context = LoopContext {
        loop_id: unique_id("loop"),
        run_id: unique_id("run"),
    };
    update_loop(store, &context, "RUNNING", None)?;
    Ok(context)
}

fn active_loop(state: &crate::state::WorkflowState) -> Result<LoopContext, SddError> {
    let active = state
        .active_loop
        .as_ref()
        .ok_or_else(|| SddError::new("E_INVALID_PHASE_COMMAND", "当前没有可控制的 auto loop"))?;
    let loop_id = active
        .get("loopId")
        .and_then(|value| value.as_str())
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "activeLoop 缺少 loopId"))?;
    let run_id = active
        .get("runId")
        .and_then(|value| value.as_str())
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "activeLoop 缺少 runId"))?;
    crate::state::state_store::validate_run_id(run_id)?;
    Ok(LoopContext {
        loop_id: loop_id.to_string(),
        run_id: run_id.to_string(),
    })
}

fn update_loop(
    store: &StateStore,
    context: &LoopContext,
    status: &str,
    waiting: Option<&str>,
) -> Result<(), SddError> {
    let cwd = store
        .sdd_dir()
        .parent()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "无法解析项目根目录"))?
        .to_string_lossy()
        .to_string();
    // 给 update_loop 一个短等待超时：瞬时锁冲突时短暂重试，避免直接 E_CONCURRENT_RUN 脆断。
    let _guard = crate::state::file_lock::lock_sdd(&cwd, "sdd auto state", None, Some(5_000))?;
    let now = crate::state::state_store::now_iso();
    let active = json!({
        "loopId": context.loop_id,
        "runId": context.run_id,
        "status": status,
        "waiting": waiting.map(|reason| json!({ "reason": reason, "since": now })),
    });
    store.update(|state| state.active_loop = Some(active.clone()))?;
    crate::state::runtime_store::write_loop_run(
        &cwd,
        &context.run_id,
        json!({
            "schemaVersion": "1.3.0",
            "loopId": context.loop_id,
            "runId": context.run_id,
            "status": status,
            "updatedAt": now,
        }),
    )
}

fn append_event(
    cwd: &str,
    context: &LoopContext,
    event_type: &str,
    phase: &str,
    command: Option<&str>,
) -> Result<(), SddError> {
    let _guard = crate::state::file_lock::lock_sdd(cwd, "sdd auto event", None, Some(5_000))?;
    crate::state::runtime_store::append_loop_event(
        cwd,
        &context.run_id,
        json!({
            "schemaVersion": "1.0.0",
            "eventId": unique_id("event"),
            "loopId": context.loop_id,
            "runId": context.run_id,
            "type": event_type,
            "phase": phase,
            "command": command,
            "createdAt": crate::state::state_store::now_iso(),
        }),
    )
}

fn read_events(
    cwd: &str,
    state: &crate::state::WorkflowState,
    args: &serde_json::Value,
) -> Result<CommandResult, SddError> {
    let context = active_loop(state)?;
    let run_id = args
        .get("run")
        .and_then(|value| value.as_str())
        .unwrap_or(&context.run_id);
    crate::state::state_store::validate_run_id(run_id)?;
    let mut events = crate::state::runtime_store::read_loop_events(cwd, run_id)?;
    if let Some(tail) = args.get("tail").and_then(|value| value.as_u64()) {
        let keep = usize::try_from(tail).unwrap_or(usize::MAX);
        if keep < events.len() {
            events.drain(..events.len() - keep);
        }
    }
    Ok(CommandResult {
        ok: true,
        state: state.current_phase.clone(),
        exit_code: 0,
        change_id: state.current_change_id.clone(),
        next: None,
        data: Some(json!({ "runId": run_id, "events": events })),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}

fn loop_status_result(state: &crate::state::WorkflowState) -> CommandResult {
    CommandResult {
        ok: true,
        state: state.current_phase.clone(),
        exit_code: 0,
        change_id: state.current_change_id.clone(),
        next: None,
        data: Some(json!({ "activeLoop": state.active_loop })),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    }
}

fn unique_id(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{prefix}-{}-{nanos}-{sequence}", std::process::id())
}
