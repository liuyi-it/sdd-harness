//! archive 命令：将审核文档整合为 archive.md，并把归档模型写入 runtime.json。
//!
//! 归档后 `.sdd/changes/<id>/` 只保留 archive.md；所有机器归档数据留在 runtime.changes。

use std::fs;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::commands::plan::plan_tasks;
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::quality::report::Report;
use crate::state::artifact_store::ArtifactRecord;
use crate::state::file_lock::lock_initialized_sdd;
use crate::state::StateStore;

pub fn run_archive(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "changeId"])?;
    let timeout_ms = super::timeout_ms(args)?;
    let _guard = lock_initialized_sdd(cwd, "sdd archive", None, timeout_ms)?;

    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let state = runtime.state.clone();
    super::ensure_phase(cwd, &state, "archive", args)?;
    let store = StateStore::new(cwd.to_string());
    let business_cwd = state
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.worktree_path.clone())
        .unwrap_or_else(|| cwd.to_string());
    let change_id = current_change_id(&state)?;
    let change_dir = crate::state::paths::change_dir(cwd, &change_id, false)?;
    let archive_path = change_dir.join("archive.md");
    crate::safe_fs::reject_symlink(&archive_path, "archive.md")?;
    let change = runtime
        .changes
        .get(&change_id)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少当前变更"))?;

    if archive_path.exists() && change.get("archive").is_some() {
        crate::state::artifact_store::verify_artifacts_in(
            cwd,
            &runtime,
            [format!("{change_id}:archive")],
        )?;
        cleanup_change_dir(&change_dir)?;
        store.update(|s| {
            s.current_phase = "ARCHIVED".to_string();
            s.clear_failure();
            s.suggested_command = Some("sdd new <需求>".to_string());
            s.last_command = Some("sdd archive".to_string());
        })?;
        return Ok(archived_result(change_id));
    }

    let spec = require_file(&change_dir, "spec.md")?;
    let plan_md = require_file(&change_dir, "plan.md")?;
    let tasks_md = require_file(&change_dir, "tasks.md")?;
    let spec_value = change
        .get("spec")
        .cloned()
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 spec"))?;
    let design = change
        .get("design")
        .cloned()
        .and_then(|value| value.as_str().map(String::from))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 design"))?;
    let plan = change
        .get("plan")
        .cloned()
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 plan"))?;
    let reports = change
        .get("reports")
        .cloned()
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
    if crate::git::GitInspector::is_git_repo(&business_cwd)? {
        let expected = review_report
            .minimality
            .as_ref()
            .and_then(|value| value.get("gitFingerprint"))
            .and_then(|value| value.as_str());
        let current = crate::git::GitInspector::workspace_fingerprint(&business_cwd)?;
        if expected != Some(current.as_str()) {
            store.update(|s| {
                s.current_phase = "BUILD_READY".to_string();
                s.record_failure("sdd archive", "审查后工作区发生变化");
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

    let tasks = plan_tasks(&plan)?;
    crate::commands::build::validate_runtime_task_state(&state, &tasks)?;
    let task_results = validated_task_results(&runtime, state.current_run_id.as_deref(), &tasks)?;
    crate::state::artifact_store::verify_artifacts_in(
        cwd,
        &runtime,
        [
            format!("{change_id}:spec"),
            format!("{change_id}:design"),
            format!("{change_id}:plan"),
            format!("{change_id}:plan-md"),
            format!("{change_id}:tasks-md"),
            format!("{change_id}:verify-report"),
            format!("{change_id}:review-report"),
        ],
    )?;

    let git = if crate::git::GitInspector::is_git_repo(&business_cwd)? {
        let workspace = state
            .workspace
            .as_ref()
            .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "Git 工作流缺少基线 workspace"))?;
        let current_head = crate::git::GitInspector::head(&business_cwd)?;
        Some(json!({
            "baselineCommit": workspace.baseline_commit,
            "currentCommit": current_head,
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
                .ok_or_else(|| {
                    SddError::new("E_STATE_CORRUPTED", "runtime.json 的 spec 缺少 requirement")
                })?
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
    crate::safe_fs::atomic_write(&archive_path, archive_md.as_bytes(), "archive.md")?;
    let artifact_key = format!("{change_id}:archive");
    let content_path = format!("runtime://changes/{change_id}/archive");
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        let change = super::change_mut(document, &change_id)?;
        change.insert("archive".to_string(), archive_value);
        crate::state::artifact_store::record_artifacts_in(
            cwd,
            document,
            vec![ArtifactRecord {
                key: &artifact_key,
                artifact_type: "summary",
                content_path: &content_path,
                inputs: json!({ "verifyPassed": true, "reviewPassed": true }),
            }],
        )?;
        crate::state::state_store::apply_state_update(&mut document.state, |state| {
            state.current_phase = "ARCHIVED".to_string();
            state.in_progress_phase = None;
            state.clear_failure();
            state.suggested_command = Some("sdd new <需求>".to_string());
            state.last_command = Some("sdd archive".to_string());
        })?;
        Ok(())
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
        next: Some("sdd new <需求>".to_string()),
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

fn validated_task_results(
    runtime: &crate::state::RuntimeDocument,
    run_id: Option<&str>,
    planned_tasks: &[crate::engines::superpowers::protocol::TaskDefinition],
) -> Result<Vec<serde_json::Value>, SddError> {
    let run_id =
        run_id.ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "状态缺少 currentRunId"))?;
    let tasks = runtime
        .runs
        .get(run_id)
        .and_then(|run| run.get("tasks"))
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "runtime.json 缺少运行任务结果对象"))?;
    if tasks.len() != planned_tasks.len() {
        return Err(SddError::new(
            "E_VERIFY_REQUIRED",
            "归档前任务结果数量与计划不一致",
        ));
    }
    planned_tasks
        .iter()
        .map(|task| {
            let result = tasks.get(&task.id).ok_or_else(|| {
                SddError::new("E_VERIFY_REQUIRED", &format!("缺少任务结果：{}", task.id))
            })?;
            let parsed = crate::protocol::validate_task_result(result)?;
            if parsed.status != "completed" {
                return Err(SddError::new(
                    "E_VERIFY_REQUIRED",
                    &format!("任务 {} 未完成", task.id),
                ));
            }
            crate::commands::build::validate_task_evidence(task, &parsed)?;
            Ok(result.clone())
        })
        .collect()
}

fn cleanup_change_dir(change_dir: &std::path::Path) -> Result<(), SddError> {
    for entry in fs::read_dir(change_dir)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("读取 change 目录失败：{e}")))?
    {
        let entry = entry.map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("读取 change 条目失败：{e}"))
        })?;
        let name = entry.file_name();
        if name == std::ffi::OsStr::new("archive.md") {
            continue;
        }
        let path = entry.path();
        let name = name.to_string_lossy();
        let file_type = entry.file_type().map_err(|e| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("读取归档制品 {name} 类型失败：{e}"),
            )
        })?;
        let result = if file_type.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
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
