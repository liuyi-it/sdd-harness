//! status 命令：只读展示项目状态和所有进行中的 change。

use crate::contracts::{CliWarning, CommandResult};
use crate::error::SddError;

/// 各稳定阶段建议的下一步命令。
pub fn next_command(phase: &str) -> Option<String> {
    let next = match phase {
        "NOT_INITIALIZED" => "sdd init",
        "INDEX_READY" => "sdd spec <需求>",
        "SPEC_WAITING_AGENT" => "sdd spec",
        "SPEC_READY" => "sdd plan",
        "PLAN_WAITING_AGENT" => "sdd plan",
        "PLAN_READY" | "BUILD_WAITING_AGENT" => "sdd build next",
        "BUILD_READY" => "sdd verify",
        "QUALITY_WAITING_FIX" => "sdd verify",
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
        let workflow = super::workflow(&document, change_id).map_err(|_| {
            SddError::new("E_MISSING_CHANGE", &format!("变更不存在：{change_id}"))
                .with_next("sdd status")
        })?;
        return Ok(result(
            &workflow.phase,
            Some(change_id.to_string()),
            qualified_next(&workflow.phase, change_id),
            Some(serde_json::json!({
                "workflow": workflow,
                "selectedChange": change_summary(&document, change_id, workflow),
                "report": quality_report(&document, change_id, workflow),
                "activeChanges": summaries,
            })),
            warnings(state),
        ));
    }

    match active.as_slice() {
        [] => Ok(result(
            "INDEX_READY",
            None,
            Some("sdd spec <需求>".to_string()),
            Some(serde_json::json!({ "activeChanges": [] })),
            warnings(state),
        )),
        [(change_id, workflow)] => Ok(result(
            &workflow.phase,
            Some((*change_id).to_string()),
            qualified_next(&workflow.phase, change_id),
            Some(serde_json::json!({
                "workflow": workflow,
                "selectedChange": change_summary(&document, change_id, workflow),
                "report": quality_report(&document, change_id, workflow),
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

pub(crate) fn qualified_next(phase: &str, change_id: &str) -> Option<String> {
    next_command(phase).map(|command| {
        if command == "sdd init" || command.starts_with("sdd spec <") {
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
    serde_json::json!({
        "changeId": change_id,
        "title": change_title(document, change_id, workflow),
        "phase": workflow.phase,
        "next": qualified_next(&workflow.phase, change_id),
        "updatedAt": workflow.updated_at,
    })
}

pub(crate) fn change_title<'a>(
    document: &'a crate::state::RuntimeDocument,
    change_id: &'a str,
    workflow: &crate::state::state_store::ChangeWorkflow,
) -> &'a str {
    let goal = document
        .changes
        .get(change_id)
        .and_then(|change| change.get("spec"))
        .and_then(|spec| spec.get("goal"))
        .and_then(serde_json::Value::as_str);
    let input = document
        .runs
        .get(&workflow.run_id)
        .and_then(|run| run.get("input"))
        .and_then(serde_json::Value::as_str);
    if workflow.phase == "SPEC_WAITING_AGENT" {
        input.or(goal)
    } else {
        goal.or(input)
    }
    .unwrap_or(change_id)
}

fn quality_report<'a>(
    document: &'a crate::state::RuntimeDocument,
    change_id: &str,
    workflow: &crate::state::state_store::ChangeWorkflow,
) -> Option<&'a serde_json::Value> {
    if !matches!(
        workflow.phase.as_str(),
        "QUALITY_WAITING_FIX" | "QUALITY_BLOCKED" | "QUALITY_READY"
    ) {
        return None;
    }
    document
        .changes
        .get(change_id)?
        .get("reports")?
        .get("quality")
}

/// 状态展示与错误引导共用中文阶段名称。
pub fn phase_label(phase: &str) -> &str {
    match phase {
        "NOT_INITIALIZED" => "尚未初始化",
        "INITIALIZING" => "正在初始化",
        "INDEX_READY" => "已就绪，可以开始新需求",
        "SPEC_WAITING_AGENT" => "等待规格与技术设计",
        "SPEC_READY" => "规格与技术设计已完成",
        "PLAN_WAITING_AGENT" => "等待实施计划",
        "PLAN_READY" => "计划已就绪，等待实施",
        "BUILD_WAITING_AGENT" => "任务实施中",
        "BUILD_READY" => "实施已完成，等待验证",
        "QUALITY_WAITING_FIX" => "等待质量修复",
        "QUALITY_BLOCKED" => "质量检查阻断，需要决定下一步",
        "QUALITY_READY" => "质量检查已通过，可以归档",
        "ARCHIVED" => "已归档",
        "MULTIPLE_CHANGES" => "有多个进行中的变更",
        _ => phase,
    }
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
