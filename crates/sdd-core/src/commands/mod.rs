//! 命令实现层：每个公共命令的领域逻辑。

pub mod archive;
pub mod auto;
pub mod build;
pub mod change;
pub mod codebase;
pub mod design;
pub mod init;
pub mod new;
pub mod plan;
pub mod review;
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

pub(crate) fn u64_arg(
    args: Option<&serde_json::Value>,
    name: &str,
) -> Result<Option<u64>, crate::error::SddError> {
    match args.and_then(|value| value.get(name)) {
        None => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or_else(|| {
            crate::error::SddError::new(
                "E_INVALID_PHASE_COMMAND",
                &format!("{name} 必须是非负整数"),
            )
        }),
    }
}

pub(crate) fn validate_string_map_arg(
    args: Option<&serde_json::Value>,
    name: &str,
) -> Result<(), crate::error::SddError> {
    let Some(value) = args.and_then(|value| value.get(name)) else {
        return Ok(());
    };
    let object = value.as_object().ok_or_else(|| {
        crate::error::SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("{name} 必须是 JSON 对象"),
        )
    })?;
    if let Some((key, _)) = object.iter().find(|(_, value)| !value.is_string()) {
        return Err(crate::error::SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("{name}.{key} 必须是字符串"),
        ));
    }
    Ok(())
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

/// 在已持有 `.sdd` 写锁并读取最新 runtime 后执行公共状态门禁。
pub(crate) fn ensure_phase(
    cwd: &str,
    state: &crate::state::WorkflowState,
    command: &str,
    args: Option<&serde_json::Value>,
) -> Result<(), crate::error::SddError> {
    check_auto_loop_busy(cwd, state, command, args)?;
    let phase = state.current_phase.as_str();
    if phase == "NOT_INITIALIZED" {
        return Err(crate::error::SddError::new(
            "E_NOT_INITIALIZED",
            "请先运行 sdd init 再执行其他命令",
        )
        .with_next("sdd init"));
    }
    if phase == "ARCHIVED" && command != "archive" && command != "new" && command != "auto" {
        return Err(crate::error::SddError::new(
            "E_ARCHIVED_READONLY",
            "已归档的变更为只读状态",
        ));
    }

    if let Some(requested) = args
        .and_then(|value| value.get("changeId"))
        .and_then(serde_json::Value::as_str)
    {
        crate::git::isolation::validate_change_id(requested)?;
        let starts_new_change =
            matches!(command, "new" | "auto") && matches!(phase, "INDEX_READY" | "ARCHIVED");
        if !starts_new_change {
            let active = state.current_change_id.as_deref().ok_or_else(|| {
                crate::error::SddError::new("E_MISSING_CHANGE", "当前没有活动变更")
                    .with_next("sdd new")
            })?;
            if requested != active {
                return Err(crate::error::SddError::new(
                    "E_MISSING_CHANGE",
                    &format!("指定变更 {requested} 不是当前活动变更 {active}"),
                ));
            }
        }
    }

    let allowed = match command {
        "change" => matches!(
            phase,
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
        "build" => matches!(phase, "PLAN_READY" | "BUILD_WAITING_AGENT" | "BUILD_READY"),
        "verify" => phase == "BUILD_READY",
        "review" => phase == "VERIFY_READY",
        "archive" => phase == "REVIEW_READY" || phase == "ARCHIVED",
        _ => true,
    };
    if !allowed {
        let next = status::next_command(phase).unwrap_or_else(|| "sdd status".to_string());
        return Err(crate::error::SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("命令 {command} 在状态 {phase} 下不可用"),
        )
        .with_next(&next));
    }
    Ok(())
}

pub(crate) fn check_auto_loop_busy(
    cwd: &str,
    state: &crate::state::WorkflowState,
    command: &str,
    args: Option<&serde_json::Value>,
) -> Result<(), crate::error::SddError> {
    let Some(active) = state.active_loop.as_ref() else {
        return Ok(());
    };
    let status = active
        .get("status")
        .and_then(serde_json::Value::as_str)
        .expect("state schema 已验证 activeLoop.status");
    if status != "RUNNING" && status != "WAITING_AGENT" {
        return Ok(());
    }
    if crate::state::file_lock::current_thread_holds_auto_lock(cwd)? {
        return Ok(());
    }
    let busy = match command {
        "init" | "new" | "change" | "design" | "plan" => true,
        "codebase" => matches!(
            args.and_then(|value| value.get("sub"))
                .and_then(serde_json::Value::as_str),
            Some("index") | Some("rebuild")
        ),
        _ => false,
    };
    if busy {
        return Err(crate::error::SddError::new(
            "E_CONCURRENT_RUN",
            "auto loop 正在运行（RUNNING/WAITING_AGENT），请等待其结束或查看进度",
        )
        .with_next("sdd auto --events"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{bool_arg, string_arg, timeout_ms, u64_arg, validate_args};

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
        assert_eq!(u64_arg(Some(&args), "value").unwrap(), Some(1));
    }
}
