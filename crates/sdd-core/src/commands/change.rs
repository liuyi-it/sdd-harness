//! change 命令：修订既有 change 的原始需求，并重新进入规格阶段。

use serde_json::{json, Value};

use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::state::file_lock::lock_initialized_sdd;
use crate::state::state_store::apply_workflow_update;

pub fn run_change(cwd: &str, args: Option<&Value>) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "changeId", "requirement", "resultJson"])?;
    let timeout_ms = super::timeout_ms(args)?;
    let requested = super::string_arg(args, "changeId")?;
    let _guard = lock_initialized_sdd(cwd, "sdd change", requested, timeout_ms)?;
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let change_id = super::resolve_change_id(&runtime, args)?;
    let workflow = super::workflow(&runtime, &change_id)?;
    super::ensure_phase(workflow, "change", &change_id)?;

    if let Some(raw) = super::string_arg(args, "resultJson")? {
        if workflow.phase != "SPEC_WAITING_AGENT"
            || workflow.last_command.as_deref() != Some("sdd change")
        {
            return Err(SddError::new(
                "E_INVALID_PHASE_COMMAND",
                "必须先执行 sdd change <新需求> 获取规格修订行动",
            ));
        }
        return super::spec::complete_spec(cwd, &runtime, &change_id, raw);
    }

    let requirement = super::string_arg(args, "requirement")?
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if requirement.is_none()
        && workflow.phase == "SPEC_WAITING_AGENT"
        && workflow.last_command.as_deref() == Some("sdd change")
    {
        return super::spec::phase_action(&runtime, &change_id, "SPECIFICATION");
    }
    let requirement =
        requirement.ok_or_else(|| SddError::new("E_INVALID_REQUIREMENT", "修订需求不能为空"))?;
    super::spec::validate_requirement_length(requirement)?;
    let run_id = workflow.run_id.clone();
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        document
            .runs
            .get_mut(&run_id)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "change workflow 缺少 run"))?
            .insert("input".to_string(), json!(requirement));
        let workflow = super::workflow_mut(document, &change_id)?;
        apply_workflow_update(workflow, |workflow| {
            workflow.phase = "SPEC_WAITING_AGENT".to_string();
            workflow.in_progress_phase = Some("SPECIFICATION".to_string());
            workflow.pending_agent_action = Some(json!({
                "type": "AGENT_PHASE_EXECUTION",
                "phase": "SPECIFICATION",
                "since": crate::state::state_store::now_iso(),
            }));
            workflow.tasks.clear();
            workflow.quality_fix_rounds = 0;
            workflow.suggested_command = Some(format!(
                "sdd change --change {change_id} --result-json '<JSON>'"
            ));
            workflow.last_command = Some("sdd change".to_string());
            workflow.clear_failure();
        })
    })?;
    let current = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    super::spec::phase_action(&current, &change_id, "SPECIFICATION")
}
