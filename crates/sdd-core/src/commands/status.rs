//! status 命令：只读展示项目状态和所有进行中的 change。

use crate::contracts::{CliWarning, CommandResult};
use crate::error::SddError;

/// 各稳定阶段建议的下一步命令。
pub fn next_command(phase: &str) -> Option<String> {
    let next = match phase {
        "NOT_INITIALIZED" => "sdd init",
        "INDEX_READY" => "sdd new <需求>",
        "SPEC_WAITING_AGENT" => "sdd new --result-json '<JSON>'",
        "SPEC_READY" => "sdd design",
        "DESIGN_WAITING_AGENT" => "sdd design --result-json '<JSON>'",
        "DESIGN_READY" => "sdd plan",
        "PLAN_WAITING_AGENT" => "sdd plan --result-json '<JSON>'",
        "PLAN_READY" | "BUILD_WAITING_AGENT" => "sdd build next",
        "BUILD_READY" => "sdd verify",
        "QUALITY_WAITING_FIX" => "sdd verify --result-json '<JSON>'",
        "QUALITY_BLOCKED" => "sdd verify --continue",
        "QUALITY_READY" => "sdd archive",
        _ => return None,
    };
    Some(next.to_string())
}

pub fn run_status(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["changeId"])?;
    let document = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let state = &document.state;
    if !state.initialized {
        return Ok(result(
            "NOT_INITIALIZED",
            None,
            Some("sdd init".to_string()),
            None,
            warnings(state),
        ));
    }

    let active = super::active_changes(&document);
    let summaries = active
        .iter()
        .map(|(change_id, workflow)| change_summary(&document, change_id, workflow))
        .collect::<Vec<_>>();

    if let Some(change_id) = super::string_arg(args, "changeId")? {
        let workflow = super::workflow(&document, change_id)
            .map_err(|_| SddError::new("E_MISSING_CHANGE", &format!("变更不存在：{change_id}")))?;
        return Ok(result(
            &workflow.phase,
            Some(change_id.to_string()),
            qualified_next(&workflow.phase, change_id),
            Some(serde_json::json!({
                "workflow": workflow,
                "activeChanges": summaries,
            })),
            warnings(state),
        ));
    }

    match active.as_slice() {
        [] => Ok(result(
            "INDEX_READY",
            None,
            Some("sdd new <需求>".to_string()),
            Some(serde_json::json!({ "activeChanges": [] })),
            warnings(state),
        )),
        [(change_id, workflow)] => Ok(result(
            &workflow.phase,
            Some((*change_id).to_string()),
            qualified_next(&workflow.phase, change_id),
            Some(serde_json::json!({
                "workflow": workflow,
                "activeChanges": summaries,
            })),
            warnings(state),
        )),
        _ => Ok(result(
            "MULTIPLE_CHANGES",
            None,
            None,
            Some(serde_json::json!({ "activeChanges": summaries })),
            warnings(state),
        )),
    }
}

fn result(
    phase: &str,
    change_id: Option<String>,
    next: Option<String>,
    data: Option<serde_json::Value>,
    warnings: Option<Vec<CliWarning>>,
) -> CommandResult {
    CommandResult {
        ok: true,
        state: phase.to_string(),
        exit_code: 0,
        change_id,
        next,
        data,
        rendered: None,
        warnings,
        action_required: None,
        error: None,
    }
}

fn warnings(state: &crate::state::WorkflowState) -> Option<Vec<CliWarning>> {
    state.degraded.then(|| {
        vec![CliWarning::new(
            "W_KNOWLEDGE_UNAVAILABLE",
            format!(
                "当前代码库索引处于降级模式：{}",
                state.degraded_reason.as_deref().unwrap_or("原因未知")
            ),
        )
        .with_next("sdd codebase doctor")]
    })
}

fn qualified_next(phase: &str, change_id: &str) -> Option<String> {
    next_command(phase).map(|command| {
        if command == "sdd init" || command.starts_with("sdd new <") {
            command
        } else {
            let suffix = command.strip_prefix("sdd ").unwrap_or(&command);
            format!("sdd {suffix} --change {change_id}")
        }
    })
}

fn change_summary(
    document: &crate::state::RuntimeDocument,
    change_id: &str,
    workflow: &crate::state::state_store::ChangeWorkflow,
) -> serde_json::Value {
    let title = document
        .changes
        .get(change_id)
        .and_then(|change| change.get("spec"))
        .and_then(|spec| spec.get("goal"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            document
                .runs
                .get(&workflow.run_id)
                .and_then(|run| run.get("input"))
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or(change_id);
    serde_json::json!({
        "changeId": change_id,
        "title": title,
        "phase": workflow.phase,
        "next": qualified_next(&workflow.phase, change_id),
        "updatedAt": workflow.updated_at,
    })
}

/// 供 CLI 错误渲染使用的项目级状态查询。
pub fn read_phase(cwd: &str) -> Result<String, SddError> {
    let document = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    if !document.state.initialized {
        return Ok("NOT_INITIALIZED".to_string());
    }
    Ok(match super::active_changes(&document).as_slice() {
        [] => "INDEX_READY".to_string(),
        [(_, workflow)] => workflow.phase.clone(),
        _ => "MULTIPLE_CHANGES".to_string(),
    })
}
