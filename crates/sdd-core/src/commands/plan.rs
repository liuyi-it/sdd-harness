//! plan 命令：生成任务计划（plan.json + plan.md）。
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
    let _guard = lock_sdd(cwd, "sdd plan", None, timeout_ms)?;

    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    let change_id = current_change_id(&state)?;
    let change_dir = PathBuf::from(cwd).join(".sdd/changes").join(&change_id);

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
        .unwrap_or_else(|_| "（代码库摘要不可用）".to_string());

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
        "contextPacks": artifacts.context_packs,
    });
    fs::write(
        change_dir.join("plan.json"),
        serde_json::to_string_pretty(&plan_json).map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("序列化 plan.json 失败：{e}"))
        })?,
    )
    .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 plan.json 失败：{e}")))?;
    fs::write(change_dir.join("plan.md"), &artifacts.tasks_markdown)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 plan.md 失败：{e}")))?;

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
