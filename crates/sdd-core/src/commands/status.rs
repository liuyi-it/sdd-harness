//! status 命令：唯一纯只读的公共命令。
//!
//! - 未初始化时返回 NOT_INITIALIZED
//! - 已初始化时原样回报当前持久化状态和建议下一步命令

use crate::contracts::{CliWarning, CommandResult};
use crate::error::SddError;
use crate::state::StateStore;

/// 各稳定阶段建议的下一步命令。
pub fn next_command(phase: &str) -> Option<String> {
    let next = match phase {
        "NOT_INITIALIZED" => "sdd init",
        "INDEX_READY" => "sdd new",
        "CLARIFYING" => "sdd new",
        "NEW_STARTED" => "sdd auto --resume",
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
    super::validate_args(args, &[])?;
    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    if !state.initialized {
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
    let next = if state.current_phase == "PAUSED" {
        state.suggested_command.clone()
    } else {
        next_command(&state.current_phase)
    };

    let mut warnings = Vec::new();
    if state.degraded {
        warnings.push(
            CliWarning::new(
                "W_KNOWLEDGE_UNAVAILABLE",
                format!(
                    "当前处于降级模式（degraded mode）：{}",
                    state
                        .degraded_reason
                        .as_deref()
                        .expect("降级状态必须包含原因")
                ),
            )
            .with_next("sdd codebase doctor"),
        );
    }

    // 序列化失败说明状态损坏，直接传播错误而不是静默降级为空对象。
    let data = serde_json::to_value(&state)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化工作流状态失败：{e}")))?;

    Ok(CommandResult {
        ok: true,
        state: state.current_phase,
        exit_code: 0,
        change_id: state.current_change_id,
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
    Ok(store.read()?.current_phase)
}
