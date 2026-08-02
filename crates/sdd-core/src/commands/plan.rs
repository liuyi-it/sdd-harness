//! plan 命令：生成单一事实源 plan.json。
//!
//! 翻译自 Node 版 `packages/core/src/commands/plan.ts`：
//! 读取 spec/design，经 TddEngine 生成原子任务链，状态推进 PLAN_READY。

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::contracts::CommandResult;
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
    for key in [
        format!("{change_id}:spec"),
        format!("{change_id}:spec-md"),
        format!("{change_id}:design"),
    ] {
        crate::state::artifact_store::verify_artifact(cwd, &key)?;
    }

    let spec = fs::read_to_string(change_dir.join("spec.md"))
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 spec.md 失败：{e}")))?;
    let design = fs::read_to_string(change_dir.join("design.md"))
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 design.md 失败：{e}")))?;
    let spec_json_raw = fs::read_to_string(change_dir.join("spec.json"))
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 spec.json 失败：{e}")))?;
    let spec_json: serde_json::Value = serde_json::from_str(&spec_json_raw)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("spec.json 解析失败：{e}")))?;
    let impact = spec_json
        .get("impact")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let index_dir = PathBuf::from(cwd).join(".sdd/index");
    let codebase_summary = fs::read_to_string(index_dir.join("codebase-summary.md"))
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取代码库摘要失败：{e}")))?;
    let spec_digest = crate::policies::digest::digest(&spec_json_raw);
    let design_digest = crate::policies::digest::digest(&design);

    let artifacts = engine.generate_plan(&PlanningInputRust {
        spec,
        design,
        impact,
        codebase_summary,
    })?;

    // 写 plan.json（任务列表 + 可读计划 + 测试计划 + 上下文摘要）
    let plan_json = json!({
        "schemaVersion": "2.0.0",
        "changeId": change_id,
        "tasks": artifacts.tasks,
        "tasksMarkdown": artifacts.tasks_markdown,
        "testPlan": artifacts.test_plan,
        "context": artifacts.context,
        "dependencies": dependencies,
    });
    let plan_text = serde_json::to_string_pretty(&plan_json)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化 plan.json 失败：{e}")))?;
    fs::write(change_dir.join("plan.json"), &plan_text)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 plan.json 失败：{e}")))?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:plan"),
        "plan",
        &format!(".sdd/changes/{change_id}/plan.json"),
        &plan_text,
        json!({
            "spec": spec_digest,
            "design": design_digest,
        }),
    )?;
    store.update(|s| {
        s.current_phase = "PLAN_READY".to_string();
        s.in_progress_phase = None;
        s.suggested_command = Some("sdd build next".to_string());
        s.last_command = Some("sdd plan".to_string());
        s.last_error = None;
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

/// 读取 plan.json 的任务列表（供 build 命令复用）
pub fn read_plan_tasks(
    cwd: &str,
    change_id: &str,
) -> Result<Vec<crate::engines::superpowers::protocol::TaskDefinition>, SddError> {
    let path = PathBuf::from(cwd)
        .join(".sdd/changes")
        .join(change_id)
        .join("plan.json");
    let raw = fs::read_to_string(&path)
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 plan.json 失败：{e}")))?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("plan.json 解析失败：{e}")))?;
    let tasks = value
        .get("tasks")
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "plan.json 缺少 tasks 字段"))?;
    serde_json::from_value(tasks.clone())
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("tasks 解析失败：{e}")))
}
