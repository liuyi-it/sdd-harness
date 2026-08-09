//! auto 命令：自动推进 SDD Loop。
//!
//! 翻译自 早期 Node 实现 + loop-engine 的核心语义：
//! - 确定性步骤（new→design→plan→build→verify→review→archive）自动推进
//! - 遇到澄清（CLARIFYING）或 Agent 编码（BUILD_WAITING_AGENT）时暂停，
//!   返回当前状态与原因，不绕过交互边界
//! - 失败预算：单步失败即暂停（stopOnFailure）

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use serde_json::json;

use crate::contracts::CommandResult;
use crate::engines::spec::spec_engine::SpecEngine;
use crate::engines::tdd::TddEngine;
use crate::error::SddError;
use crate::state::StateStore;

pub fn run_auto(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    let empty_args = serde_json::Value::Null;
    let args = args.unwrap_or(&empty_args);
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
        append_event(cwd, &context, "LOOP_STOPPED", &state.current_phase, None)?;
        return Ok(paused_result_with_reason(
            &state.current_phase,
            "auto loop 已停止；工作流阶段未改变",
        ));
    }

    let context = prepare_loop(cwd, &store, &state, resume, restart, args)?;
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

    // 第一步：new（无需求且未初始化变更 → 暂停）
    let mut phase = state.current_phase.clone();
    if phase == "INDEX_READY" || phase == "CLARIFYING" || phase == "FAILED" || phase == "PAUSED" {
        let Some(requirement) = requirement.clone() else {
            update_loop(&store, &context, "PAUSED", Some("CLARIFICATION"))?;
            append_event(cwd, &context, "LOOP_PAUSED", &phase, None)?;
            return Ok(paused_result_with_reason(
                &phase,
                "auto 需要需求文本（sdd auto \"<需求>\"）",
            ));
        };
        let mut new_args =
            serde_json::Map::from_iter([("requirement".to_string(), json!(requirement))]);
        for key in ["changeId", "nonInteractive", "timeout"] {
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
                "需求存在未回答的阻塞问题，请用 sdd new --answers '<JSON>' 继续",
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
                        timeout_ms,
                    )?;
                    store.update(|s| {
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
    cwd: &str,
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
    let loop_dir = PathBuf::from(cwd).join(".sdd/loop");
    fs::create_dir_all(loop_dir.join("runs"))
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("创建 loop 目录失败：{e}")))?;
    fs::write(
        loop_dir.join("loop.json"),
        serde_json::to_string_pretty(&json!({
            "schemaVersion": "1.3.0",
            "loopId": context.loop_id,
            "mode": "auto",
            "maxSteps": 20,
            "maxRetriesPerStep": 1,
            "createdAt": crate::state::state_store::now_iso(),
        }))
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化 loop 失败：{e}")))?,
    )
    .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 loop 失败：{e}")))?;
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
    let sdd_dir = store.sdd_dir();
    let cwd = sdd_dir
        .parent()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "无法解析项目根目录"))?
        .to_string_lossy()
        .to_string();
    let _guard = crate::state::file_lock::lock_sdd(&cwd, "sdd auto state", None, None)?;
    let now = crate::state::state_store::now_iso();
    let active = json!({
        "loopId": context.loop_id,
        "runId": context.run_id,
        "status": status,
        "waiting": waiting.map(|reason| json!({ "reason": reason, "since": now })),
    });
    store.update(|state| state.active_loop = Some(active.clone()))?;
    let run_path = store
        .sdd_dir()
        .join("loop/runs")
        .join(format!("{}.json", context.run_id));
    if let Some(parent) = run_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("创建 loop run 目录失败：{e}"))
        })?;
    }
    fs::write(
        run_path,
        serde_json::to_string_pretty(&json!({
            "schemaVersion": "1.3.0",
            "loopId": context.loop_id,
            "runId": context.run_id,
            "status": status,
            "updatedAt": now,
        }))
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化 loop run 失败：{e}")))?,
    )
    .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 loop run 失败：{e}")))
}

fn append_event(
    cwd: &str,
    context: &LoopContext,
    event_type: &str,
    phase: &str,
    command: Option<&str>,
) -> Result<(), SddError> {
    let dir = PathBuf::from(cwd).join(".sdd/loop/events");
    fs::create_dir_all(&dir)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("创建 loop 事件目录失败：{e}")))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{}.jsonl", context.run_id)))
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("打开 loop 事件失败：{e}")))?;
    let event = json!({
        "schemaVersion": "1.0.0",
        "eventId": unique_id("event"),
        "loopId": context.loop_id,
        "runId": context.run_id,
        "type": event_type,
        "phase": phase,
        "command": command,
        "createdAt": crate::state::state_store::now_iso(),
    });
    writeln!(file, "{event}")
        .and_then(|_| file.sync_all())
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 loop 事件失败：{e}")))
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
    let path = PathBuf::from(cwd)
        .join(".sdd/loop/events")
        .join(format!("{run_id}.jsonl"));
    let raw = fs::read_to_string(&path)
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 loop 事件失败：{e}")))?;
    let mut events: Vec<serde_json::Value> = raw
        .lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|e| {
                SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("loop 事件第 {} 行损坏：{e}", index + 1),
                )
            })
        })
        .collect::<Result<_, _>>()?;
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
    format!("{prefix}-{nanos}")
}
