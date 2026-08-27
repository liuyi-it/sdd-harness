//! auto 命令：自动推进 SDD Loop。
//!
//! 组合命令按当前 loop 状态机推进：
//! - 确定性步骤（new→design→plan→build→verify→review→archive）自动推进
//! - 遇到澄清（CLARIFYING）或 Agent 编码（BUILD_WAITING_AGENT）时暂停，
//!   返回当前状态与原因，不绕过交互边界
//! - 失败预算：单步失败即暂停

use serde_json::json;

use crate::contracts::CommandResult;

use crate::engines::spec::spec_engine::SpecEngine;
use crate::engines::tdd::TddEngine;
use crate::error::SddError;

pub fn run_auto(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    super::validate_args(
        args,
        &[
            "timeout",
            "changeId",
            "nonInteractive",
            "requirement",
            "answers",
            "resume",
            "restart",
            "stop",
            "events",
            "tail",
            "loopStatus",
            "run",
        ],
    )?;
    let empty_args = serde_json::Value::Null;
    let args = args.unwrap_or(&empty_args);
    for name in [
        "nonInteractive",
        "resume",
        "restart",
        "stop",
        "events",
        "loopStatus",
    ] {
        super::bool_arg(Some(args), name)?;
    }
    super::string_arg(Some(args), "requirement")?;
    super::string_arg(Some(args), "run")?;
    super::u64_arg(Some(args), "tail")?;
    super::validate_string_map_arg(Some(args), "answers")?;
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
    let timeout_ms = super::timeout_ms(Some(args))?;
    let _sdd_guard = crate::state::file_lock::lock_initialized_sdd(
        cwd,
        "sdd auto",
        args.get("changeId").and_then(|value| value.as_str()),
        timeout_ms,
    )?;
    let _auto_guard = crate::state::file_lock::lock_auto(
        cwd,
        "sdd auto",
        args.get("changeId").and_then(|value| value.as_str()),
        timeout_ms,
    )?;
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let state = runtime.state.clone();
    super::ensure_phase(cwd, &state, "auto", Some(args))?;
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
        return read_events(&runtime, &state, args);
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
        persist_transition(
            cwd,
            &context,
            "ABORTED",
            None,
            [LoopEvent::new("LOOP_STOPPED", &state.current_phase, None)],
            |workflow| {
                // 停止只终止 loop，不改变工作流阶段；恢复时回到原有 Agent/命令边界。
                workflow.in_progress_phase = None;
                workflow.suggested_command = Some("sdd auto --resume".to_string());
            },
        )?;
        return Ok(paused_result_with_reason(
            &state.current_phase,
            "auto loop 已停止；工作流阶段未改变",
        ));
    }

    let context = prepare_loop(&state, resume, restart, args)?;
    persist_transition(
        cwd,
        &context,
        "RUNNING",
        None,
        [LoopEvent::new(
            if resume {
                "LOOP_RESUMED"
            } else {
                "LOOP_STARTED"
            },
            &state.current_phase,
            None,
        )],
        |_| {},
    )?;

    let requirement = args
        .get("requirement")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let spec_engine = SpecEngine::new();

    // 第一步：new。INDEX_READY 必须提供需求，其它可恢复阶段从 runtime 读取原始需求。
    // ARCHIVED 且提供了新需求时也要走 new 开启新变更；无需求保持走 archive 幂等分支。
    let mut phase = state.current_phase;
    if matches!(
        phase.as_str(),
        "INDEX_READY" | "CLARIFYING" | "PAUSED" | "NEW_STARTED"
    ) || (phase == "ARCHIVED" && requirement.is_some())
    {
        if phase == "INDEX_READY" && requirement.is_none() {
            persist_transition(
                cwd,
                &context,
                "PAUSED",
                Some("CLARIFICATION"),
                [LoopEvent::new("LOOP_PAUSED", &phase, None)],
                |_| {},
            )?;
            return Ok(paused_result_with_reason(
                &phase,
                "auto 需要需求文本（sdd auto \"<需求>\"）",
            ));
        }
        let mut new_args = serde_json::Map::new();
        if let Some(requirement) = requirement {
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
            persist_transition(
                cwd,
                &context,
                "PAUSED",
                Some("CLARIFICATION"),
                [LoopEvent::new("LOOP_PAUSED", "CLARIFYING", Some("new"))],
                |_| {},
            )?;
            return Ok(paused_result_with_reason(
                "CLARIFYING",
                "需求存在未回答的阻塞问题，请用 sdd auto --resume --answers '<JSON>' 继续",
            ));
        }
        phase = new_result.state;
    }

    // 下游命令只接收自身支持的公共参数，auto 控制字段不得泄漏并被静默忽略。
    let mut step_args = serde_json::Map::new();
    for key in ["timeout", "changeId"] {
        if let Some(value) = args.get(key) {
            step_args.insert(key.to_string(), value.clone());
        }
    }
    let step_args = serde_json::Value::Object(step_args);

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
        let should_run = match name {
            "design" => phase == "SPEC_READY",
            "plan" => phase == "DESIGN_READY",
            // build 只在等待/可派发任务时运行；BUILD_READY（任务已全部完成）跳过
            "build" => phase == "PLAN_READY" || phase == "BUILD_WAITING_AGENT",
            "verify" => phase == "BUILD_READY",
            "review" => phase == "VERIFY_READY",
            "archive" => phase == "REVIEW_READY" || phase == "ARCHIVED",
            _ => false,
        };
        if !should_run {
            continue;
        }
        match step(cwd, Some(&step_args)) {
            Ok(result) => {
                if result.state == "BUILD_WAITING_AGENT" {
                    // Agent 编码边界：暂停，返回 actionRequired
                    persist_transition(
                        cwd,
                        &context,
                        "WAITING_AGENT",
                        Some("AGENT_TASK_EXECUTION"),
                        [
                            LoopEvent::new("COMMAND_FINISHED", &result.state, Some(name)),
                            LoopEvent::new("ACTION_REQUIRED", &result.state, Some(name)),
                        ],
                        |_| {},
                    )?;
                    return Ok(result);
                }
                if result.state == "ARCHIVED" {
                    persist_transition(
                        cwd,
                        &context,
                        "SUCCEEDED",
                        None,
                        [
                            LoopEvent::new("COMMAND_FINISHED", "ARCHIVED", Some(name)),
                            LoopEvent::new("LOOP_ARCHIVED", "ARCHIVED", Some(name)),
                        ],
                        |_| {},
                    )?;
                    return Ok(CommandResult {
                        ok: true,
                        state: "ARCHIVED".to_string(),
                        exit_code: 0,
                        change_id: result.change_id,
                        next: Some("sdd new <需求>".to_string()),
                        data: Some(json!({ "loop": "COMPLETED" })),
                        rendered: None,
                        warnings: None,
                        action_required: None,
                        error: None,
                    });
                }
                phase = result.state;
                persist_events(
                    cwd,
                    &context,
                    [LoopEvent::new("COMMAND_FINISHED", &phase, Some(name))],
                )?;
            }
            Err(e) => {
                persist_transition(
                    cwd,
                    &context,
                    "FAILED",
                    None,
                    [LoopEvent::new("LOOP_FAILED", &phase, Some(name))],
                    |workflow| {
                        // 步骤失败时将工作流阶段置为 PAUSED，供 status 给出恢复建议。
                        workflow.current_phase = "PAUSED".to_string();
                        workflow.in_progress_phase = None;
                        workflow.record_failure(format!("sdd {name}"), e.message.clone());
                        workflow.suggested_command = Some("sdd auto --resume".to_string());
                    },
                )?;
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

    persist_transition(
        cwd,
        &context,
        "PAUSED",
        None,
        [LoopEvent::new("LOOP_PAUSED", &phase, None)],
        |_| {},
    )?;
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

#[derive(Clone, Copy)]
struct LoopEvent<'a> {
    event_type: &'a str,
    phase: &'a str,
    command: Option<&'a str>,
}

impl<'a> LoopEvent<'a> {
    fn new(event_type: &'a str, phase: &'a str, command: Option<&'a str>) -> Self {
        Self {
            event_type,
            phase,
            command,
        }
    }
}

fn prepare_loop(
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
        return Ok(context);
    }
    let starts_new_change = state.current_phase == "ARCHIVED"
        && args
            .get("requirement")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|requirement| !requirement.trim().is_empty());
    if !restart && state.active_loop.is_some() && !starts_new_change {
        return active_loop(state);
    }
    Ok(LoopContext {
        loop_id: crate::state::state_store::unique_id("loop")?,
        run_id: crate::state::state_store::unique_id("run")?,
    })
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
    crate::state::state_store::validate_run_id(loop_id)?;
    crate::state::state_store::validate_run_id(run_id)?;
    Ok(LoopContext {
        loop_id: loop_id.to_string(),
        run_id: run_id.to_string(),
    })
}

fn persist_transition<'a, I, F>(
    cwd: &str,
    context: &LoopContext,
    status: &str,
    waiting: Option<&str>,
    events: I,
    update_workflow: F,
) -> Result<(), SddError>
where
    I: IntoIterator<Item = LoopEvent<'a>>,
    F: FnOnce(&mut crate::state::WorkflowState),
{
    let event_values = loop_event_values(context, events)?;
    let _guard = crate::state::file_lock::lock_sdd(cwd, "sdd auto state", None, Some(5_000))?;
    let now = crate::state::state_store::now_iso();
    let active = json!({
        "loopId": context.loop_id,
        "runId": context.run_id,
        "status": status,
        "waiting": waiting.map(|reason| json!({ "reason": reason, "since": now })),
    });
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        crate::state::state_store::apply_state_update(&mut document.state, |state| {
            state.active_loop = Some(active);
            update_workflow(state);
        })?;
        set_loop_run(
            document,
            &context.run_id,
            json!({
            "schemaVersion": "1.3.0",
            "loopId": context.loop_id,
            "runId": context.run_id,
            "status": status,
            "updatedAt": now,
            }),
        )?;
        append_loop_events(document, &context.run_id, event_values)?;
        Ok(())
    })?;
    Ok(())
}

fn persist_events<'a, I>(cwd: &str, context: &LoopContext, events: I) -> Result<(), SddError>
where
    I: IntoIterator<Item = LoopEvent<'a>>,
{
    let event_values = loop_event_values(context, events)?;
    let _guard = crate::state::file_lock::lock_sdd(cwd, "sdd auto event", None, Some(5_000))?;
    crate::state::RuntimeStore::new(cwd.to_string())
        .try_update(|document| append_loop_events(document, &context.run_id, event_values))?;
    Ok(())
}

fn loop_event_values<'a, I>(
    context: &LoopContext,
    events: I,
) -> Result<Vec<serde_json::Value>, SddError>
where
    I: IntoIterator<Item = LoopEvent<'a>>,
{
    events
        .into_iter()
        .map(|event| {
            Ok(json!({
            "schemaVersion": "1.0.0",
                "eventId": crate::state::state_store::unique_id("event")?,
            "loopId": context.loop_id,
            "runId": context.run_id,
                "type": event.event_type,
                "phase": event.phase,
                "command": event.command,
            "createdAt": crate::state::state_store::now_iso(),
            }))
        })
        .collect()
}

fn set_loop_run(
    document: &mut crate::state::RuntimeDocument,
    run_id: &str,
    run: serde_json::Value,
) -> Result<(), SddError> {
    let runs = document
        .loop_state
        .get_mut("runs")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "runtime.loop.runs 必须是对象"))?;
    runs.insert(run_id.to_string(), run);
    Ok(())
}

fn append_loop_events(
    document: &mut crate::state::RuntimeDocument,
    run_id: &str,
    event_values: Vec<serde_json::Value>,
) -> Result<(), SddError> {
    let events = document
        .loop_state
        .get_mut("events")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "runtime.loop.events 必须是对象"))?;
    let run_events = events
        .entry(run_id.to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "auto loop 事件必须是数组"))?;
    run_events.extend(event_values);
    Ok(())
}

fn read_events(
    runtime: &crate::state::RuntimeDocument,
    state: &crate::state::WorkflowState,
    args: &serde_json::Value,
) -> Result<CommandResult, SddError> {
    let context = active_loop(state)?;
    let run_id = args
        .get("run")
        .and_then(|value| value.as_str())
        .unwrap_or(&context.run_id);
    crate::state::state_store::validate_run_id(run_id)?;
    let events = runtime
        .loop_state
        .get("events")
        .and_then(|events| events.get(run_id))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "指定 auto run 没有事件"))?;
    let start = if let Some(tail) = args.get("tail").and_then(|value| value.as_u64()) {
        let keep = usize::try_from(tail).map_err(|_| {
            SddError::new("E_INVALID_PHASE_COMMAND", "--tail 超出当前平台可表示范围")
        })?;
        events.len().saturating_sub(keep)
    } else {
        0
    };
    Ok(CommandResult {
        ok: true,
        state: state.current_phase.clone(),
        exit_code: 0,
        change_id: state.current_change_id.clone(),
        next: None,
        data: Some(json!({ "runId": run_id, "events": &events[start..] })),
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
