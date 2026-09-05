//! 命令实现层：每个公共命令的领域逻辑。

pub mod archive;
pub mod build;
pub mod change;
pub mod codebase;
pub mod init;
pub mod plan;
pub mod spec;
pub mod status;
pub mod verify;

pub(crate) fn validate_args(
    args: Option<&serde_json::Value>,
    allowed: &[&str],
) -> Result<(), crate::error::SddError> {
    let Some(args) = args else {
        return Ok(());
    };
    let fields = args.as_object().ok_or_else(|| {
        crate::error::SddError::new("E_INVALID_PHASE_COMMAND", "命令参数必须是 JSON 对象")
    })?;
    let mut unknown = fields
        .keys()
        .filter(|field| !allowed.contains(&field.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if unknown.is_empty() {
        if let Some(change_id) = string_arg(Some(args), "changeId")? {
            if change_id.trim().is_empty() {
                return Err(crate::error::SddError::new(
                    "E_INVALID_PHASE_COMMAND",
                    "changeId 不得为空",
                ));
            }
            crate::git::isolation::validate_change_id(change_id)?;
        }
        return Ok(());
    }
    unknown.sort();
    Err(crate::error::SddError::new(
        "E_INVALID_PHASE_COMMAND",
        &format!("命令包含未知参数：{}", unknown.join("、")),
    ))
}

pub(crate) fn string_arg<'a>(
    args: Option<&'a serde_json::Value>,
    name: &str,
) -> Result<Option<&'a str>, crate::error::SddError> {
    match args.and_then(|value| value.get(name)) {
        None => Ok(None),
        Some(value) => value.as_str().map(Some).ok_or_else(|| {
            crate::error::SddError::new("E_INVALID_PHASE_COMMAND", &format!("{name} 必须是字符串"))
        }),
    }
}

pub(crate) fn bool_arg(
    args: Option<&serde_json::Value>,
    name: &str,
) -> Result<Option<bool>, crate::error::SddError> {
    match args.and_then(|value| value.get(name)) {
        None => Ok(None),
        Some(value) => value.as_bool().map(Some).ok_or_else(|| {
            crate::error::SddError::new("E_INVALID_PHASE_COMMAND", &format!("{name} 必须是布尔值"))
        }),
    }
}

pub(crate) fn timeout_ms(
    args: Option<&serde_json::Value>,
) -> Result<Option<u64>, crate::error::SddError> {
    let Some(raw) = args.and_then(|value| value.get("timeout")) else {
        return Ok(None);
    };
    let seconds = raw
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| {
            crate::error::SddError::new("E_INVALID_PHASE_COMMAND", "timeout 必须是非负有限数字")
        })?;
    let duration = std::time::Duration::try_from_secs_f64(seconds).map_err(|_| {
        crate::error::SddError::new("E_INVALID_PHASE_COMMAND", "timeout 超出支持范围")
    })?;
    let milliseconds = u64::try_from(duration.as_millis()).map_err(|_| {
        crate::error::SddError::new("E_INVALID_PHASE_COMMAND", "timeout 超出支持范围")
    })?;
    Ok(Some(milliseconds))
}

pub(crate) fn change_mut<'a>(
    document: &'a mut crate::state::RuntimeDocument,
    change_id: &str,
) -> Result<&'a mut serde_json::Map<String, serde_json::Value>, crate::error::SddError> {
    document
        .changes
        .get_mut(change_id)
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| crate::error::SddError::new("E_STATE_CORRUPTED", "当前变更必须是对象"))
}

pub(crate) fn reports_mut(
    change: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<&mut serde_json::Map<String, serde_json::Value>, crate::error::SddError> {
    change
        .entry("reports".to_string())
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            crate::error::SddError::new("E_STATE_CORRUPTED", "runtime.json 的 reports 必须是对象")
        })
}

pub(crate) fn ensure_initialized(
    state: &crate::state::WorkflowState,
) -> Result<(), crate::error::SddError> {
    if !state.initialized {
        return Err(crate::error::SddError::new(
            "E_NOT_INITIALIZED",
            "请先运行 sdd init 再执行其他命令",
        )
        .with_next("sdd init"));
    }
    Ok(())
}

pub(crate) fn active_changes(
    document: &crate::state::RuntimeDocument,
) -> Vec<(&str, &crate::state::state_store::ChangeWorkflow)> {
    document
        .workflows
        .iter()
        .filter(|(_, workflow)| workflow.phase != "ARCHIVED")
        .map(|(change_id, workflow)| (change_id.as_str(), workflow))
        .collect()
}

/// 解析当前命令的 change。没有显式 change 时只允许唯一活动任务，绝不猜测最近任务。
pub(crate) fn resolve_change_id(
    document: &crate::state::RuntimeDocument,
    args: Option<&serde_json::Value>,
) -> Result<String, crate::error::SddError> {
    ensure_initialized(&document.state)?;
    if let Some(change_id) = string_arg(args, "changeId")? {
        crate::git::isolation::validate_change_id(change_id)?;
        if document.workflows.contains_key(change_id) {
            return Ok(change_id.to_string());
        }
        return Err(crate::error::SddError::new(
            "E_MISSING_CHANGE",
            &format!("变更不存在：{change_id}"),
        )
        .with_next("sdd status"));
    }
    let active = active_changes(document);
    match active.as_slice() {
        [] => Err(
            crate::error::SddError::new("E_MISSING_CHANGE", "当前没有进行中的 SDD 任务")
                .with_next("sdd spec <需求>"),
        ),
        [(change_id, _)] => Ok((*change_id).to_string()),
        _ => Err(crate::error::SddError::new(
            "E_CHANGE_SELECTION_REQUIRED",
            &format!(
                "当前有多个进行中的任务，请选择目标并传入 --change <标识>：\n{}",
                active
                    .iter()
                    .map(|(change_id, workflow)| format!(
                        "- {} [{}]：{}",
                        status::change_title(document, change_id, workflow),
                        change_id,
                        status::phase_label(&workflow.phase)
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        )
        .with_next("sdd status")),
    }
}

pub(crate) fn workflow<'a>(
    document: &'a crate::state::RuntimeDocument,
    change_id: &str,
) -> Result<&'a crate::state::state_store::ChangeWorkflow, crate::error::SddError> {
    document
        .workflows
        .get(change_id)
        .ok_or_else(|| crate::error::SddError::new("E_STATE_CORRUPTED", "change 缺少 workflow"))
}

pub(crate) fn workflow_mut<'a>(
    document: &'a mut crate::state::RuntimeDocument,
    change_id: &str,
) -> Result<&'a mut crate::state::state_store::ChangeWorkflow, crate::error::SddError> {
    document
        .workflows
        .get_mut(change_id)
        .ok_or_else(|| crate::error::SddError::new("E_STATE_CORRUPTED", "change 缺少 workflow"))
}

pub(crate) fn ensure_phase(
    workflow: &crate::state::state_store::ChangeWorkflow,
    command: &str,
    change_id: &str,
) -> Result<(), crate::error::SddError> {
    let phase = workflow.phase.as_str();
    if phase == "ARCHIVED" && command != "archive" && command != "change" {
        return Err(
            crate::error::SddError::new("E_ARCHIVED_READONLY", "已归档的变更为只读状态")
                .with_next(&format!("sdd status --change {change_id}")),
        );
    }
    let allowed = match command {
        "change" => matches!(
            phase,
            "SPEC_WAITING_AGENT"
                | "SPEC_READY"
                | "PLAN_WAITING_AGENT"
                | "PLAN_READY"
                | "BUILD_WAITING_AGENT"
                | "BUILD_READY"
                | "QUALITY_WAITING_FIX"
                | "QUALITY_BLOCKED"
                | "QUALITY_READY"
                | "ARCHIVED"
        ),
        "spec" => phase == "SPEC_WAITING_AGENT",
        "plan" => matches!(phase, "SPEC_READY" | "PLAN_WAITING_AGENT"),
        "build" => matches!(phase, "PLAN_READY" | "BUILD_WAITING_AGENT" | "BUILD_READY"),
        "verify" => matches!(
            phase,
            "BUILD_READY" | "QUALITY_WAITING_FIX" | "QUALITY_BLOCKED" | "QUALITY_READY"
        ),
        "archive" => phase == "QUALITY_READY" || phase == "ARCHIVED",
        _ => true,
    };
    if !allowed {
        let next = status::qualified_next(phase, change_id)
            .unwrap_or_else(|| format!("sdd status --change {change_id}"));
        return Err(crate::error::SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!(
                "当前阶段：{}；暂时不能执行 sdd {command}",
                status::phase_label(phase)
            ),
        )
        .with_next(&next));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{bool_arg, string_arg, timeout_ms, validate_args};

    #[test]
    fn timeout_is_parsed_once_with_range_checks() {
        assert_eq!(timeout_ms(None).unwrap(), None);
        assert_eq!(
            timeout_ms(Some(&serde_json::json!({ "timeout": 0.5 }))).unwrap(),
            Some(500)
        );
        for invalid in [
            serde_json::json!({ "timeout": -1 }),
            serde_json::json!({ "timeout": "1" }),
            serde_json::json!({ "timeout": 1e30 }),
        ] {
            assert_eq!(
                timeout_ms(Some(&invalid)).unwrap_err().code,
                "E_INVALID_PHASE_COMMAND"
            );
        }
    }

    #[test]
    fn named_arguments_reject_wrong_types() {
        let args = serde_json::json!({ "changeId": 1 });
        assert_eq!(
            validate_args(Some(&args), &["changeId"]).unwrap_err().code,
            "E_INVALID_PHASE_COMMAND"
        );
        let args = serde_json::json!({ "value": 1 });
        assert!(string_arg(Some(&args), "value").is_err());
        assert!(bool_arg(Some(&args), "value").is_err());
    }
}
