//! archive 命令：在统一质量门禁通过后生成单一归档文档。

use std::fs;

use serde_json::{json, Value};

use crate::commands::plan::plan_tasks;
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::quality::report::Report;
use crate::state::artifact_store::ArtifactRecord;
use crate::state::file_lock::lock_initialized_sdd;
use crate::state::state_store::{apply_workflow_update, TASK_STATUS_DONE};

pub fn run_archive(cwd: &str, args: Option<&Value>) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "changeId"])?;
    let timeout_ms = super::timeout_ms(args)?;
    let _guard = lock_initialized_sdd(cwd, timeout_ms)?;
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let change_id = super::resolve_change_id(&runtime, args)?;
    let workflow = super::workflow(&runtime, &change_id)?;
    super::ensure_phase(workflow, "archive", &change_id)?;
    if workflow.phase == "ARCHIVED" {
        crate::state::artifact_store::verify_artifacts_in(
            cwd,
            &runtime,
            [format!("{change_id}:archive")],
        )?;
        return Ok(archived_result(change_id));
    }

    let change = runtime
        .changes
        .get(&change_id)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime 缺少 change"))?;
    let report: Report = serde_json::from_value(
        change
            .get("reports")
            .and_then(|reports| reports.get("quality"))
            .cloned()
            .ok_or_else(|| SddError::new("E_QUALITY_REQUIRED", "缺少质量报告"))?,
    )
    .map_err(|error| SddError::new("E_STATE_CORRUPTED", &error.to_string()))?;
    if report.kind != "quality" || !report.passed {
        return Err(
            SddError::new("E_QUALITY_REQUIRED", "质量报告未通过，禁止归档")
                .with_next(&format!("sdd verify --change {change_id}")),
        );
    }
    let business_cwd = workflow
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.worktree_path.clone())
        .unwrap_or_else(|| cwd.to_string());
    if crate::git::GitInspector::is_git_repo(&business_cwd)? {
        let expected = report
            .minimality
            .as_ref()
            .and_then(|value| value.get("gitFingerprint"))
            .and_then(Value::as_str);
        let current = crate::git::GitInspector::workspace_fingerprint(&business_cwd)?;
        if expected != Some(current.as_str()) {
            crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
                let workflow = super::workflow_mut(document, &change_id)?;
                apply_workflow_update(workflow, |workflow| {
                    workflow.phase = "BUILD_READY".to_string();
                    workflow.record_failure("sdd archive", "质量验证后工作区发生变化");
                    workflow.suggested_command = Some(format!("sdd verify --change {change_id}"));
                })
            })?;
            return Err(SddError::new(
                "E_QUALITY_REQUIRED",
                "质量验证后工作区发生变化，请重新执行 sdd verify",
            )
            .with_next(&format!("sdd verify --change {change_id}")));
        }
    }

    let plan = change
        .get("plan")
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime 缺少 plan"))?;
    let tasks = plan_tasks(plan)?;
    crate::commands::build::validate_runtime_task_state(workflow, &tasks)?;
    if workflow
        .tasks
        .values()
        .any(|status| status != TASK_STATUS_DONE)
    {
        return Err(SddError::new(
            "E_VERIFY_REQUIRED",
            "存在未完成任务，禁止归档",
        ));
    }
    let task_results = runtime
        .runs
        .get(&workflow.run_id)
        .and_then(|run| run.get("tasks"))
        .cloned()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "run 缺少任务结果"))?;
    let change_dir = crate::state::paths::change_dir(cwd, &change_id, false)?;
    let spec = require_file(&change_dir, "spec.md")?;
    let plan_md = require_file(&change_dir, "plan.md")?;
    let tasks_md = require_file(&change_dir, "tasks.md")?;
    let quality_md = require_file(&change_dir, "quality-report.md")?;
    crate::state::artifact_store::verify_artifacts_in(
        cwd,
        &runtime,
        [
            format!("{change_id}:spec"),
            format!("{change_id}:plan"),
            format!("{change_id}:plan-md"),
            format!("{change_id}:tasks-md"),
            format!("{change_id}:quality-report"),
        ],
    )?;

    let archived_at = crate::state::state_store::now_iso();
    let archive_md = format!(
        "# 需求归档\n\n- 变更：{change_id}\n- 归档时间：{archived_at}\n- 已完成任务数：{}\n\n{spec}\n\n{plan_md}\n\n{tasks_md}\n\n{quality_md}",
        tasks.len()
    );
    let git = if crate::git::GitInspector::is_git_repo(&business_cwd)? {
        let changed_files = workflow
            .workspace
            .as_ref()
            .map(|workspace| {
                crate::git::GitInspector::changes_since(
                    &business_cwd,
                    &workspace.baseline_changed_files,
                    &workspace.baseline_file_hashes,
                )
            })
            .transpose()?
            .unwrap_or_default();
        Some(json!({
            "currentCommit": crate::git::GitInspector::head(&business_cwd)?,
            "changedFiles": changed_files,
        }))
    } else {
        None
    };
    let archive_value = json!({
        "schemaVersion": "3.0.0",
        "changeId": change_id,
        "spec": change.get("spec"),
        "plan": plan,
        "taskResults": task_results,
        "qualityReport": report,
        "git": git,
        "archivedAt": archived_at,
    });
    crate::safe_fs::atomic_write(
        &change_dir.join("archive.md"),
        archive_md.as_bytes(),
        "archive.md",
    )?;
    let artifact_key = format!("{change_id}:archive");
    let content_path = format!("runtime://changes/{change_id}/archive");
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        super::change_mut(document, &change_id)?
            .insert("archive".to_string(), archive_value.clone());
        crate::state::artifact_store::record_artifacts_in(
            cwd,
            document,
            vec![ArtifactRecord {
                key: &artifact_key,
                artifact_type: "summary",
                content_path: &content_path,
                inputs: json!({ "qualityPassed": true }),
            }],
        )?;
        let workflow = super::workflow_mut(document, &change_id)?;
        apply_workflow_update(workflow, |workflow| {
            workflow.phase = "ARCHIVED".to_string();
            workflow.in_progress_phase = None;
            workflow.pending_agent_action = None;
            workflow.suggested_command = Some("sdd status".to_string());
            workflow.last_command = Some("sdd archive".to_string());
            workflow.clear_failure();
        })
    })?;
    cleanup_change_dir(&change_dir)?;
    Ok(archived_result(change_id))
}

fn archived_result(change_id: String) -> CommandResult {
    CommandResult {
        ok: true,
        state: "ARCHIVED".to_string(),
        exit_code: 0,
        change_id: Some(change_id),
        next: Some("sdd status".to_string()),
        data: None,
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    }
}

fn require_file(change_dir: &std::path::Path, name: &str) -> Result<String, SddError> {
    let path = change_dir.join(name);
    crate::safe_fs::reject_symlink(&path, name)?;
    fs::read_to_string(path)
        .map_err(|error| SddError::new("E_MISSING_ARTIFACT", &format!("读取 {name} 失败：{error}")))
}

fn cleanup_change_dir(change_dir: &std::path::Path) -> Result<(), SddError> {
    for entry in fs::read_dir(change_dir)
        .map_err(|error| SddError::new("E_STATE_CORRUPTED", &error.to_string()))?
    {
        let entry =
            entry.map_err(|error| SddError::new("E_STATE_CORRUPTED", &error.to_string()))?;
        if entry.file_name() == std::ffi::OsStr::new("archive.md") {
            continue;
        }
        let path = entry.path();
        crate::safe_fs::reject_symlink(&path, "归档前清理")?;
        let kind = entry
            .file_type()
            .map_err(|error| SddError::new("E_STATE_CORRUPTED", &error.to_string()))?;
        if kind.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
        .map_err(|error| {
            SddError::new("E_STATE_CORRUPTED", &format!("清理归档文档失败：{error}"))
        })?;
    }
    Ok(())
}
