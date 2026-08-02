//! status 命令：唯一纯只读的公共命令。
//!
//! - 未初始化时返回 NOT_INITIALIZED
//! - 已初始化时原样回报当前持久化状态和建议下一步命令
//!
//! 翻译自 Node 版 `packages/core/src/commands/status.ts`。

use serde_json::json;

use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::state::{StateStore, WorkflowState};

/// 各阶段建议的下一步命令（与 Node 版 NEXT_BY_PHASE 一致）
pub fn next_command(phase: &str) -> Option<String> {
    let next = match phase {
        "NOT_INITIALIZED" => "sdd init",
        "INDEX_READY" => "sdd new",
        "CLARIFYING" => "sdd new",
        "SPEC_READY" => "sdd design",
        "DESIGN_READY" => "sdd plan",
        "PLAN_READY" => "sdd build next",
        "BUILD_READY" => "sdd verify",
        "VERIFY_READY" => "sdd review",
        "REVIEW_READY" => "sdd archive",
        _ => return None,
    };
    Some(next.to_string())
}

pub fn run_status(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    let store = StateStore::new(cwd.to_string());
    if !store.state_path().exists() {
        return Ok(CommandResult {
            ok: true,
            state: "NOT_INITIALIZED".to_string(),
            exit_code: 0,
            change_id: None,
            next: Some("sdd init".to_string()),
            data: None,
            rendered: None,
            warnings: None,
            action_required: None,
            error: None,
        });
    }
    let state = store.read()?;
    let next = if state.current_phase == "FAILED" || state.current_phase == "PAUSED" {
        state.suggested_command.clone()
    } else {
        next_command(&state.current_phase)
    };

    let mut warnings: Vec<serde_json::Value> = Vec::new();
    if state.degraded {
        warnings.push(json!({
            "code": "W_KNOWLEDGE_UNAVAILABLE",
            "message": format!("当前处于降级模式（degraded mode）：{}",
                state.degraded_reason.as_deref().unwrap_or("知识图谱引擎不可用")),
            "next": "sdd codebase doctor",
        }));
    }

    // --loop 时返回 activeLoop 摘要
    let mut data = serde_json::to_value(&state).unwrap_or(json!({}));
    if args
        .and_then(|a| a.get("loopStatus").or_else(|| a.get("loop")))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        && state.active_loop.is_some()
    {
        if let Some(loop_value) = &state.active_loop {
            let mut obj = data.as_object().cloned().unwrap_or_default();
            obj.insert("activeLoop".to_string(), loop_value.clone());
            data = serde_json::Value::Object(obj);
        }
    }

    Ok(CommandResult {
        ok: true,
        state: state.current_phase.clone(),
        exit_code: 0,
        change_id: state.current_change_id.clone(),
        next,
        data: Some(data),
        rendered: None,
        warnings: if warnings.is_empty() {
            None
        } else {
            Some(warnings)
        },
        action_required: None,
        error: None,
    })
}

/// 供其他命令使用的纯状态查询
pub fn read_phase(cwd: &str) -> Result<String, SddError> {
    let store = StateStore::new(cwd.to_string());
    if !store.state_path().exists() {
        return Ok("NOT_INITIALIZED".to_string());
    }
    let state: WorkflowState = store.read()?;
    Ok(state.current_phase)
}
