//! archive 命令：将三份审核文档整合为 archive.md 后清理变更目录。
//!
//! 翻译自 早期 Node 实现：
//! - 重新验证报告摘要与文件范围
//! - 收敛 .sdd/changes/<id>/ 为 archive.json + archive.md + .archived
//! - 状态 ARCHIVED；中断后可再次执行收敛

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::quality::report::Report;
use crate::state::file_lock::lock_sdd;
use crate::state::StateStore;

const ARCHIVE_MARKER: &str = ".archived";

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

    // 已有归档标记 → 仅在组合哈希有效时幂等收敛到 ARCHIVED
    if change_dir.join(ARCHIVE_MARKER).exists() {
        validate_archive_marker(&change_dir)?;
        let archive_json = require_file(&change_dir, "archive.json")?;
        record_archive(cwd, &change_id, &archive_json)?;
        cleanup_change_dir(&change_dir)?;
        store.update(|s| {
            s.current_phase = "ARCHIVED".to_string();
            s.suggested_command = Some("sdd new <需求>".to_string());
            s.last_command = Some("sdd archive".to_string());
        })?;
        return Ok(CommandResult {
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
        });
    }

    let spec_json_raw = require_file(&change_dir, "spec.json")?;
    let spec_json: serde_json::Value = serde_json::from_str(&spec_json_raw)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("spec.json 解析失败：{e}")))?;
    let spec = require_file(&change_dir, "spec.md")?;
    let plan_md = require_file(&change_dir, "plan.md")?;
    let tasks_md = require_file(&change_dir, "tasks.md")?;
    let design = spec_json
        .get("design")
        .and_then(|value| value.as_str())
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "spec.json 缺少 design 字段"))?
        .to_string();
    let plan_raw = require_file(&change_dir, "plan.json")?;
    let plan: serde_json::Value = serde_json::from_str(&plan_raw)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("plan.json 解析失败：{e}")))?;
    let verify_report = require_report(&change_dir, "verify-report.json", "verify")?;
    let review_report = require_report(&change_dir, "review-report.json", "review")?;
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
        format!("{change_id}:spec-json"),
        format!("{change_id}:design"),
        format!("{change_id}:plan"),
        format!("{change_id}:plan-md"),
        format!("{change_id}:tasks-md"),
        format!("{change_id}:verify-report"),
        format!("{change_id}:review-report"),
    ] {
        crate::state::artifact_store::verify_artifact(cwd, &key)?;
    }
    let run_id = state
        .current_run_id
        .as_deref()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "状态缺少 currentRunId"))?;
    for result in &task_results {
        let task_id = result
            .get("taskId")
            .and_then(|value| value.as_str())
            .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "任务结果缺少 taskId"))?;
        crate::state::artifact_store::verify_artifact(cwd, &format!("{run_id}:{task_id}:result"))?;
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
            spec_json
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

    let archive_json = json!({
        "schemaVersion": "2.0.0",
        "changeId": change_id,
        "spec": spec,
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

    fs::write(change_dir.join("archive.md"), &archive_md)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 archive.md 失败：{e}")))?;
    let archive_json_text = serde_json::to_string_pretty(&archive_json)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化失败：{e}")))?;
    fs::write(change_dir.join("archive.json"), &archive_json_text)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 archive.json 失败：{e}")))?;

    // 写组合哈希标记
    let combined = format!("{archive_md}{archive_json_text}");
    let marker = crate::policies::digest::digest(&combined);
    fs::write(change_dir.join(ARCHIVE_MARKER), marker)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入归档标记失败：{e}")))?;
    record_archive(cwd, &change_id, &archive_json_text)?;

    cleanup_change_dir(&change_dir)?;

    store.update(|s| {
        s.current_phase = "ARCHIVED".to_string();
        s.in_progress_phase = None;
        s.suggested_command = Some("sdd new <需求>".to_string());
        s.last_command = Some("sdd archive".to_string());
        s.last_error = None;
    })?;

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
        &format!(".sdd/changes/{change_id}/archive.json"),
        content,
        json!({ "verifyPassed": true, "reviewPassed": true }),
    )
}

fn require_file(change_dir: &std::path::Path, name: &str) -> Result<String, SddError> {
    fs::read_to_string(change_dir.join(name))
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 {name} 失败：{e}")))
}

fn require_report(
    change_dir: &std::path::Path,
    name: &str,
    kind: &str,
) -> Result<Report, SddError> {
    let raw = require_file(change_dir, name)?;
    let report: Report = serde_json::from_str(&raw)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("{name} 解析失败：{e}")))?;
    if report.kind != kind {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("{name} 的 kind 应为 {kind}"),
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
    let dir = PathBuf::from(cwd)
        .join(".sdd/runs")
        .join(run_id)
        .join("tasks");
    let mut results = Vec::new();
    for entry in fs::read_dir(&dir)
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取任务结果失败：{e}")))?
    {
        let entry = entry.map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("读取任务结果条目失败：{e}"))
        })?;
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(entry.path())
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("读取任务结果失败：{e}")))?;
        let value: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("任务结果解析失败：{e}")))?;
        results.push(value);
    }
    results.sort_by_key(|value| {
        value
            .get("taskId")
            .and_then(|task_id| task_id.as_str())
            .unwrap_or("")
            .to_string()
    });
    Ok(results)
}

fn validate_archive_marker(change_dir: &std::path::Path) -> Result<(), SddError> {
    let archive_md = require_file(change_dir, "archive.md")?;
    let archive_json = require_file(change_dir, "archive.json")?;
    let marker = require_file(change_dir, ARCHIVE_MARKER)?;
    let expected = crate::policies::digest::digest(&format!("{archive_md}{archive_json}"));
    if marker.trim() != expected {
        return Err(SddError::new(
            "E_COMPONENT_INTEGRITY_FAILED",
            "归档标记与 archive.json/archive.md 内容不一致",
        ));
    }
    Ok(())
}

fn cleanup_change_dir(change_dir: &std::path::Path) -> Result<(), SddError> {
    for entry in fs::read_dir(change_dir)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("读取 change 目录失败：{e}")))?
    {
        let entry = entry.map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("读取 change 条目失败：{e}"))
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "archive.json" || name == "archive.md" || name == ARCHIVE_MARKER {
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
