//! plan 命令：生成供人工审核的 plan.md/tasks.md，并把机器计划存入 runtime.json。

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::contracts::CommandResult;
use crate::engines::superpowers::protocol::TaskDefinition;
use crate::engines::tdd::tdd_engine::{PlanningInputRust, TddEngine};
use crate::error::SddError;
use crate::state::file_lock::lock_sdd;
use crate::state::StateStore;

pub fn run_plan(
    cwd: &str,
    args: Option<&serde_json::Value>,
    engine: &TddEngine,
) -> Result<CommandResult, SddError> {
    let timeout_ms = args
        .and_then(|a| a.get("timeout"))
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0) as u64);
    let dependencies = args
        .and_then(|value| value.get("dependencies"))
        .cloned()
        .unwrap_or_else(|| json!([]));
    validate_dependencies(&dependencies)?;
    let _guard = lock_sdd(cwd, "sdd plan", None, timeout_ms)?;

    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    let change_id = current_change_id(&state)?;
    let change_dir = PathBuf::from(cwd).join(".sdd/changes").join(&change_id);
    for key in [format!("{change_id}:spec"), format!("{change_id}:design")] {
        crate::state::artifact_store::verify_artifact(cwd, &key)?;
    }

    let spec_value = crate::state::runtime_store::read_change_field(cwd, &change_id, "spec")?
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 spec"))?;
    let spec = fs::read_to_string(change_dir.join("spec.md"))
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 spec.md 失败：{e}")))?;
    let design = crate::state::runtime_store::read_change_field(cwd, &change_id, "design")?
        .or_else(|| spec_value.get("design").cloned())
        .and_then(|value| value.as_str().map(String::from))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 design 字段"))?;
    let impact = spec_value
        .get("impact")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let codebase_summary = crate::state::runtime_store::read_index_field(cwd, "summary")?
        .and_then(|value| value.as_str().map(String::from))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少代码库摘要"))?;
    let spec_digest = crate::policies::digest::digest(&spec);
    let design_digest = crate::policies::digest::digest(&design);

    let artifacts = engine.generate_plan(&PlanningInputRust {
        spec,
        design: design.clone(),
        impact,
        codebase_summary,
    })?;

    // 写任务前校验每条验证命令：任务进入 build 派发后不再拦截，
    // 必须在计划落盘时就把"任意命令"挡在门外
    for task in &artifacts.tasks {
        for verification in &task.verification {
            crate::security::verification_command::validate_verification_command(verification)?;
        }
    }

    let plan_markdown = render_plan_document(&design, &artifacts.test_plan, &dependencies);
    let tasks_markdown = artifacts.tasks_markdown.clone();
    let plan_value = json!({
        "schemaVersion": "2.0.0",
        "changeId": change_id,
        "tasks": artifacts.tasks,
        "dependencies": dependencies,
    });
    let plan_text = serde_json::to_string_pretty(&plan_value)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化机器计划失败：{e}")))?;
    fs::write(change_dir.join("plan.md"), &plan_markdown)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 plan.md 失败：{e}")))?;
    fs::write(change_dir.join("tasks.md"), &tasks_markdown)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 tasks.md 失败：{e}")))?;
    crate::state::runtime_store::write_change_field(cwd, &change_id, "plan", plan_value)?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:plan"),
        "plan",
        &format!("runtime://changes/{change_id}/plan"),
        &plan_text,
        json!({ "spec": spec_digest, "design": design_digest }),
    )?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:plan-md"),
        "plan",
        &format!(".sdd/changes/{change_id}/plan.md"),
        &plan_markdown,
        json!({ "plan": crate::policies::digest::digest(&plan_text) }),
    )?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:tasks-md"),
        "plan",
        &format!(".sdd/changes/{change_id}/tasks.md"),
        &tasks_markdown,
        json!({ "plan": crate::policies::digest::digest(&plan_text) }),
    )?;
    store.update(|s| {
        s.current_phase = "PLAN_READY".to_string();
        s.in_progress_phase = None;
        s.suggested_command = Some("sdd build next".to_string());
        s.last_command = Some("sdd plan".to_string());
        s.last_error = None;
        s.tasks.clear();
        s.pending_agent_task = None;
    })?;

    Ok(CommandResult {
        ok: true,
        state: "PLAN_READY".to_string(),
        exit_code: 0,
        change_id: Some(change_id),
        next: Some("sdd build next".to_string()),
        data: Some(json!({ "taskCount": artifacts.tasks.len() })),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}

fn render_plan_document(design: &str, test_plan: &str, dependencies: &serde_json::Value) -> String {
    let design = design.strip_prefix("# Design").unwrap_or(design).trim();
    let test_plan = test_plan
        .strip_prefix("# Test Plan")
        .unwrap_or(test_plan)
        .trim();
    let mut lines = vec![
        "# 实施计划".to_string(),
        String::new(),
        "## 技术方案与架构".to_string(),
        String::new(),
        design.to_string(),
        String::new(),
        "## 实施顺序".to_string(),
        String::new(),
        "1. 按 tasks.md 的任务依赖顺序执行。".to_string(),
        "2. 每个需求依次完成 RED、GREEN、REFACTOR、VERIFY。".to_string(),
        String::new(),
        "## 测试计划".to_string(),
        String::new(),
        test_plan.to_string(),
        String::new(),
        "## 依赖决策".to_string(),
        String::new(),
    ];
    match dependencies.as_array() {
        Some(entries) if !entries.is_empty() => {
            for entry in entries {
                let name = entry
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知依赖");
                let action = entry
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知操作");
                let reason = entry
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未说明原因");
                lines.push(format!("- {name}（{action}）：{reason}"));
            }
        }
        _ => lines.push("- 无新增依赖。".to_string()),
    }
    lines.join("\n") + "\n"
}

fn validate_dependencies(dependencies: &serde_json::Value) -> Result<(), SddError> {
    let entries = dependencies
        .as_array()
        .ok_or_else(|| SddError::new("E_INVALID_PHASE_COMMAND", "dependencies 必须是 JSON 数组"))?;
    for (index, entry) in entries.iter().enumerate() {
        let object = entry.as_object().ok_or_else(|| {
            SddError::new(
                "E_INVALID_PHASE_COMMAND",
                &format!("dependencies[{index}] 必须是对象"),
            )
        })?;
        for field in ["name", "manifest", "action", "reason"] {
            if object.get(field).and_then(|value| value.as_str()).is_none() {
                return Err(SddError::new(
                    "E_INVALID_PHASE_COMMAND",
                    &format!("dependencies[{index}].{field} 必须是字符串"),
                ));
            }
        }
        if !matches!(
            object.get("action").and_then(|value| value.as_str()),
            Some("ADD" | "UPDATE" | "REMOVE")
        ) {
            return Err(SddError::new(
                "E_INVALID_PHASE_COMMAND",
                &format!("dependencies[{index}].action 非法"),
            ));
        }
        if !object
            .get("requirements")
            .and_then(|value| value.as_array())
            .is_some_and(|values| values.iter().all(|value| value.is_string()))
        {
            return Err(SddError::new(
                "E_INVALID_PHASE_COMMAND",
                &format!("dependencies[{index}].requirements 必须是字符串数组"),
            ));
        }
    }
    Ok(())
}

/// 读取 runtime 中的计划任务（供 build、verify 和 review 复用）。
pub fn read_plan_tasks(cwd: &str, change_id: &str) -> Result<Vec<TaskDefinition>, SddError> {
    let value = crate::state::runtime_store::read_change_field(cwd, change_id, "plan")?
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 plan"))?;
    let tasks = value.get("tasks").ok_or_else(|| {
        SddError::new("E_MISSING_ARTIFACT", "runtime.json 的 plan 缺少 tasks 字段")
    })?;
    // 对每个任务执行 task.schema.json 校验（task id 格式、status/phase 枚举等），
    // 防止手工篡改 runtime 的计划任务绕过结构约束。
    if let Some(list) = tasks.as_array() {
        for (index, task) in list.iter().enumerate() {
            crate::schema::validate_json("task", task).map_err(|error| {
                SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("计划任务 {index} 校验失败：{}", error.message),
                )
            })?;
        }
    }
    serde_json::from_value(tasks.clone())
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("tasks 解析失败：{e}")))
}
