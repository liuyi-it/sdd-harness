//! archive 命令：将审核文档整合为 archive.md，并把归档模型写入 runtime.json。
//!
//! 归档后 `.sdd/changes/<id>/` 只保留 archive.md；所有机器归档数据留在 runtime.changes。

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::quality::report::Report;
use crate::state::file_lock::lock_sdd;
use crate::state::StateStore;

pub fn run_archive(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    let timeout_ms = args
        .and_then(|a| a.get("timeout"))
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0) as u64);
    let _guard = lock_sdd(cwd, "sdd archive", None, timeout_ms)?;

    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    let business_cwd = state
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.worktree_path.clone())
        .unwrap_or_else(|| cwd.to_string());
    let change_id = current_change_id(&state)?;
    let change_dir = PathBuf::from(cwd).join(".sdd/changes").join(&change_id);

    if change_dir.join("archive.md").exists()
        && crate::state::runtime_store::read_change_field(cwd, &change_id, "archive")?.is_some()
    {
        crate::state::artifact_store::verify_artifact(cwd, &format!("{change_id}:archive"))?;
        cleanup_change_dir(&change_dir)?;
        store.update(|s| {
            s.current_phase = "ARCHIVED".to_string();
            s.suggested_command = Some("sdd new <需求>".to_string());
            s.last_command = Some("sdd archive".to_string());
        })?;
        return archived_result(change_id);
    }

    let spec = require_file(&change_dir, "spec.md")?;
    let plan_md = require_file(&change_dir, "plan.md")?;
    let tasks_md = require_file(&change_dir, "tasks.md")?;
    let spec_value = crate::state::runtime_store::read_change_field(cwd, &change_id, "spec")?
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 spec"))?;
    let design = crate::state::runtime_store::read_change_field(cwd, &change_id, "design")?
        .or_else(|| spec_value.get("design").cloned())
        .and_then(|value| value.as_str().map(String::from))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 design"))?;
    let plan = crate::state::runtime_store::read_change_field(cwd, &change_id, "plan")?
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 plan"))?;
    let reports = crate::state::runtime_store::read_change_field(cwd, &change_id, "reports")?
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 reports"))?;
    let verify_report = parse_report(&reports, "verify")?;
    let review_report = parse_report(&reports, "review")?;
    if !verify_report.passed {
        return Err(
            SddError::new("E_VERIFY_REQUIRED", "验证报告未通过，禁止归档").with_next("sdd verify"),
        );
    }
    if !review_report.passed {
        return Err(
            SddError::new("E_REVIEW_REQUIRED", "审查报告未通过，禁止归档").with_next("sdd review"),
        );
    }
    if crate::git::GitInspector::is_git_repo(&business_cwd) {
        let expected = review_report
            .minimality
            .as_ref()
            .and_then(|value| value.get("gitFingerprint"))
            .and_then(|value| value.as_str());
        let current = crate::git::GitInspector::workspace_fingerprint(&business_cwd)?;
        if expected != Some(current.as_str()) {
            store.update(|s| {
                s.current_phase = "BUILD_READY".to_string();
                s.failed_command = Some("sdd archive".to_string());
                s.failed_reason = Some("审查后工作区发生变化".to_string());
                s.suggested_command = Some("sdd verify".to_string());
                s.last_command = Some("sdd archive".to_string());
            })?;
            return Err(SddError::new(
                "E_REVIEW_REQUIRED",
                "审查后工作区发生变化，请重新执行 sdd verify 与 sdd review",
            )
            .with_next("sdd verify"));
        }
    }

    let task_results = collect_task_results(cwd, state.current_run_id.as_deref())?;
    let tasks = crate::commands::plan::read_plan_tasks(cwd, &change_id)?;
    if task_results.len() != tasks.len() {
        return Err(SddError::new(
            "E_VERIFY_REQUIRED",
            "归档前任务结果数量与计划不一致",
        ));
    }
    for task in &tasks {
        let result = task_results
            .iter()
            .find(|result| {
                result.get("taskId").and_then(|value| value.as_str()) == Some(task.id.as_str())
            })
            .ok_or_else(|| {
                SddError::new("E_VERIFY_REQUIRED", &format!("缺少任务结果：{}", task.id))
            })?;
        let parsed = crate::protocol::validate_task_result(result)?;
        if parsed.status != "completed" {
            return Err(SddError::new(
                "E_VERIFY_REQUIRED",
                &format!("任务 {} 未完成", task.id),
            ));
        }
        crate::commands::build::validate_task_evidence(task, &parsed, result)?;
    }
    for key in [
        format!("{change_id}:spec"),
        format!("{change_id}:design"),
        format!("{change_id}:plan"),
        format!("{change_id}:plan-md"),
        format!("{change_id}:tasks-md"),
        format!("{change_id}:verify-report"),
        format!("{change_id}:review-report"),
    ] {
        crate::state::artifact_store::verify_artifact(cwd, &key)?;
    }

    let git = if let Some(workspace) = &state.workspace {
        let current = crate::git::GitInspector::snapshot(&business_cwd)?;
        Some(json!({
            "baselineCommit": workspace.baseline_commit,
            "currentCommit": current.head,
            "changedFiles": crate::git::GitInspector::changes_since(
                &business_cwd,
                &workspace.baseline_changed_files,
                &workspace.baseline_file_hashes,
            )?,
        }))
    } else {
        None
    };

    let archived_at = crate::state::state_store::now_iso();
    let archive_md = [
        "# 需求归档".to_string(),
        String::new(),
        format!("- 变更：{change_id}"),
        format!(
            "- 目标：{}",
            spec_value
                .get("requirement")
                .and_then(|value| value.as_str())
                .unwrap_or("未记录")
        ),
        format!("- 任务：{} 个", tasks.len()),
        String::new(),
        "## 需求规格".to_string(),
        String::new(),
        spec.trim().to_string(),
        String::new(),
        "## 实施设计".to_string(),
        String::new(),
        design.trim().to_string(),
        String::new(),
        "## 实施计划".to_string(),
        String::new(),
        plan_md.trim().to_string(),
        String::new(),
        "## 开发任务".to_string(),
        String::new(),
        tasks_md.trim().to_string(),
        String::new(),
        "## 验证结果".to_string(),
        String::new(),
        format!("- 结果：{}", verify_report.summary),
        String::new(),
        "## 审查结果".to_string(),
        String::new(),
        format!("- 结果：{}", review_report.summary),
        String::new(),
        "## 归档时间".to_string(),
        String::new(),
        archived_at.clone(),
    ]
    .join("\n")
        + "\n";
    let archive_value = json!({
        "schemaVersion": "2.0.0",
        "changeId": change_id,
        "spec": spec_value,
        "design": design,
        "plan": plan,
        "planDocument": plan_md,
        "tasksDocument": tasks_md,
        "taskResults": task_results,
        "verifyReport": verify_report,
        "reviewReport": review_report,
        "git": git,
        "archivedAt": archived_at,
    });
    let archive_text = serde_json::to_string_pretty(&archive_value)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化归档模型失败：{e}")))?;
    fs::write(change_dir.join("archive.md"), &archive_md)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 archive.md 失败：{e}")))?;
    crate::state::runtime_store::write_change_field(cwd, &change_id, "archive", archive_value)?;
    record_archive(cwd, &change_id, &archive_text)?;
    cleanup_change_dir(&change_dir)?;

    store.update(|s| {
        s.current_phase = "ARCHIVED".to_string();
        s.in_progress_phase = None;
        s.suggested_command = Some("sdd new <需求>".to_string());
        s.last_command = Some("sdd archive".to_string());
        s.last_error = None;
    })?;

    archived_result(change_id)
}

fn archived_result(change_id: String) -> Result<CommandResult, SddError> {
    Ok(CommandResult {
        ok: true,
        state: "ARCHIVED".to_string(),
        exit_code: 0,
        change_id: Some(change_id),
        next: Some("sdd new <需求>".to_string()),
        data: None,
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}

fn record_archive(cwd: &str, change_id: &str, content: &str) -> Result<(), SddError> {
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:archive"),
        "summary",
        &format!("runtime://changes/{change_id}/archive"),
        content,
        json!({ "verifyPassed": true, "reviewPassed": true }),
    )
}

fn require_file(change_dir: &std::path::Path, name: &str) -> Result<String, SddError> {
    fs::read_to_string(change_dir.join(name))
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 {name} 失败：{e}")))
}

fn parse_report(reports: &serde_json::Value, kind: &str) -> Result<Report, SddError> {
    let value = reports.get(kind).cloned().ok_or_else(|| {
        SddError::new(
            "E_MISSING_ARTIFACT",
            &format!("runtime.json 缺少 {kind} 报告"),
        )
    })?;
    let report: Report = serde_json::from_value(value)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("{kind} 报告解析失败：{e}")))?;
    if report.kind != kind {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("{kind} 报告 kind 不匹配"),
        ));
    }
    Ok(report)
}

fn collect_task_results(
    cwd: &str,
    run_id: Option<&str>,
) -> Result<Vec<serde_json::Value>, SddError> {
    let run_id =
        run_id.ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "状态缺少 currentRunId"))?;
    let tasks = crate::state::runtime_store::read_run_field(cwd, run_id, "tasks")?
        .unwrap_or_else(|| json!({}));
    let mut results: Vec<serde_json::Value> = tasks
        .as_object()
        .map(|entries| entries.values().cloned().collect())
        .unwrap_or_default();
    results.sort_by_key(|value| {
        value
            .get("taskId")
            .and_then(|task_id| task_id.as_str())
            .unwrap_or("")
            .to_string()
    });
    Ok(results)
}

fn cleanup_change_dir(change_dir: &std::path::Path) -> Result<(), SddError> {
    for entry in fs::read_dir(change_dir)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("读取 change 目录失败：{e}")))?
    {
        let entry = entry.map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("读取 change 条目失败：{e}"))
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "archive.md" {
            continue;
        }
        let result = if entry.path().is_dir() {
            fs::remove_dir_all(entry.path())
        } else {
            fs::remove_file(entry.path())
        };
        result.map_err(|e| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("清理归档制品 {name} 失败：{e}"),
            )
        })?;
    }
    Ok(())
}
