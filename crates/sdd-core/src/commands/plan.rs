//! plan 命令：生成供人工审核的 plan.md/tasks.md，并把机器计划存入 runtime.json。

use std::fs;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::contracts::CommandResult;
use crate::engines::superpowers::protocol::{PlanningInput, TaskDefinition};
use crate::engines::tdd::tdd_engine::TddEngine;
use crate::error::SddError;
use crate::state::artifact_store::ArtifactRecord;
use crate::state::file_lock::lock_initialized_sdd;

pub fn run_plan(
    cwd: &str,
    args: Option<&serde_json::Value>,
    engine: &TddEngine,
) -> Result<CommandResult, SddError> {
    let timeout_ms = super::timeout_ms(args)?;
    let _guard = lock_initialized_sdd(cwd, "sdd plan", None, timeout_ms)?;

    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let state = runtime.state.clone();
    super::ensure_phase(cwd, &state, "plan", args)?;
    super::validate_args(args, &["timeout", "changeId", "dependencies"])?;
    let dependencies = args
        .and_then(|value| value.get("dependencies"))
        .cloned()
        .unwrap_or_else(|| json!([]));
    validate_dependencies(&dependencies)?;
    let change_id = current_change_id(&state)?;
    let change_dir = crate::state::paths::change_dir(cwd, &change_id, false)?;
    crate::state::artifact_store::verify_artifacts_in(
        cwd,
        &runtime,
        [format!("{change_id}:spec"), format!("{change_id}:design")],
    )?;

    let change = runtime
        .changes
        .get(&change_id)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少当前变更"))?;
    let spec_value = change
        .get("spec")
        .cloned()
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 spec"))?;
    let spec_path = change_dir.join("spec.md");
    crate::safe_fs::reject_symlink(&spec_path, "spec.md")?;
    let spec = fs::read_to_string(spec_path)
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 spec.md 失败：{e}")))?;
    let design = change
        .get("design")
        .cloned()
        .and_then(|value| value.as_str().map(String::from))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 design 字段"))?;
    let impact = spec_value
        .get("impact")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "runtime.json 的 spec 缺少 impact"))?
        .to_string();
    let codebase_summary = runtime
        .index
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .map(String::from)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少代码库摘要"))?;
    let spec_digest = crate::policies::digest::digest(&spec);
    let design_digest = crate::policies::digest::digest(&design);

    let artifacts = engine.generate_plan(&PlanningInput {
        spec: &spec,
        design: &design,
        impact: &impact,
        codebase_summary: &codebase_summary,
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
    plan_tasks(&plan_value)?;
    let task_statuses = artifacts
        .tasks
        .iter()
        .map(|task| {
            (
                task.id.clone(),
                crate::state::state_store::TASK_STATUS_PENDING.to_string(),
            )
        })
        .collect();
    let run_id = state
        .current_run_id
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "DESIGN_READY 状态缺少 currentRunId"))?;
    let plan_text = serde_json::to_string_pretty(&plan_value)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化机器计划失败：{e}")))?;
    crate::safe_fs::atomic_write(
        &change_dir.join("plan.md"),
        plan_markdown.as_bytes(),
        "plan.md",
    )?;
    crate::safe_fs::atomic_write(
        &change_dir.join("tasks.md"),
        tasks_markdown.as_bytes(),
        "tasks.md",
    )?;
    let plan_digest = crate::policies::digest::digest(&plan_text);
    let plan_key = format!("{change_id}:plan");
    let plan_path = format!("runtime://changes/{change_id}/plan");
    let plan_md_key = format!("{change_id}:plan-md");
    let plan_md_path = format!(".sdd/changes/{change_id}/plan.md");
    let tasks_md_key = format!("{change_id}:tasks-md");
    let tasks_md_path = format!(".sdd/changes/{change_id}/tasks.md");
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        let change = super::change_mut(document, &change_id)?;
        change.insert("plan".to_string(), plan_value);
        let run = document
            .runs
            .get_mut(&run_id)
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "当前 run 必须是对象"))?;
        run.insert("tasks".to_string(), json!({}));
        crate::state::artifact_store::record_artifacts_in(
            cwd,
            document,
            vec![
                ArtifactRecord {
                    key: &plan_key,
                    artifact_type: "plan",
                    content_path: &plan_path,
                    inputs: json!({ "spec": spec_digest, "design": design_digest }),
                },
                ArtifactRecord {
                    key: &plan_md_key,
                    artifact_type: "plan",
                    content_path: &plan_md_path,
                    inputs: json!({ "plan": &plan_digest }),
                },
                ArtifactRecord {
                    key: &tasks_md_key,
                    artifact_type: "plan",
                    content_path: &tasks_md_path,
                    inputs: json!({ "plan": &plan_digest }),
                },
            ],
        )?;
        crate::state::state_store::apply_state_update(&mut document.state, |state| {
            state.current_phase = "PLAN_READY".to_string();
            state.in_progress_phase = None;
            state.clear_failure();
            state.suggested_command = Some("sdd build next".to_string());
            state.last_command = Some("sdd plan".to_string());
            state.tasks = task_statuses;
            state.pending_agent_task = None;
        })?;
        Ok(())
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
    let design = design
        .strip_prefix("# Design")
        .expect("TddEngine 设计文档必须以 # Design 开头")
        .trim();
    let test_plan = test_plan
        .strip_prefix("# Test Plan")
        .expect("TddEngine 测试计划必须以 # Test Plan 开头")
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
    let entries = dependencies
        .as_array()
        .expect("validate_dependencies 已确认 dependencies 是数组");
    if entries.is_empty() {
        lines.push("- 无新增依赖。".to_string());
    } else {
        for entry in entries {
            let name = entry["name"]
                .as_str()
                .expect("validate_dependencies 已确认 name 是字符串");
            let action = entry["action"]
                .as_str()
                .expect("validate_dependencies 已确认 action 是字符串");
            let reason = entry["reason"]
                .as_str()
                .expect("validate_dependencies 已确认 reason 是字符串");
            lines.push(format!("- {name}（{action}）：{reason}"));
        }
    }
    lines.join("\n") + "\n"
}

pub(crate) fn validate_dependencies(dependencies: &serde_json::Value) -> Result<(), SddError> {
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
        let expected = ["name", "manifest", "action", "reason", "requirements"];
        if object.len() != expected.len()
            || object
                .keys()
                .any(|field| !expected.contains(&field.as_str()))
        {
            return Err(SddError::new(
                "E_INVALID_PHASE_COMMAND",
                &format!("dependencies[{index}] 包含未知或缺失字段"),
            ));
        }
        for field in ["name", "manifest", "reason"] {
            if !object
                .get(field)
                .and_then(|value| value.as_str())
                .is_some_and(|value| !value.trim().is_empty())
            {
                return Err(SddError::new(
                    "E_INVALID_PHASE_COMMAND",
                    &format!("dependencies[{index}].{field} 必须是非空字符串"),
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
            .is_some_and(|values| {
                !values.is_empty()
                    && values
                        .iter()
                        .all(|value| value.as_str().is_some_and(|value| !value.trim().is_empty()))
            })
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
    crate::git::isolation::validate_change_id(change_id)?;
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let value = runtime
        .changes
        .get(change_id)
        .and_then(|change| change.get("plan"))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 plan"))?;
    plan_tasks(value)
}

pub(crate) fn plan_tasks(value: &serde_json::Value) -> Result<Vec<TaskDefinition>, SddError> {
    let tasks = value.get("tasks").ok_or_else(|| {
        SddError::new("E_MISSING_ARTIFACT", "runtime.json 的 plan 缺少 tasks 字段")
    })?;
    // 对每个任务执行 task.schema.json 校验（task id 格式、phase 枚举与文件范围等），
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
    let parsed: Vec<TaskDefinition> = serde_json::from_value(tasks.clone())
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("tasks 解析失败：{e}")))?;
    crate::engines::superpowers::planner::validate_task_graph(&parsed)?;
    for task in &parsed {
        let allowed: std::collections::HashSet<&str> =
            task.allowed_files.iter().map(String::as_str).collect();
        if task
            .expected_new_files
            .iter()
            .any(|path| !allowed.contains(path.as_str()))
            || !allowed.contains(task.test_seam.as_str())
        {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                &format!(
                    "任务 {} 的新增文件或 testSeam 不在 allowedFiles 中",
                    task.id
                ),
            ));
        }
        for verification in &task.verification {
            crate::security::verification_command::validate_verification_command(verification)
                .map_err(|error| {
                    SddError::new(
                        "E_STATE_CORRUPTED",
                        &format!("任务 {} 的验证命令非法：{}", task.id, error.message),
                    )
                })?;
        }
    }
    Ok(parsed)
}
