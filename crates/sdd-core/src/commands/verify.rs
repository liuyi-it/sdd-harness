//! verify 命令：验证规格、任务与证据覆盖。
//!
//! 机器报告位于 `.sdd/runtime.json`，change 目录只保留 verify-report.md。

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::commands::plan::read_plan_tasks;
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::git::GitInspector;
use crate::quality::report::{render_report_markdown, Issue, Report};
use crate::quality::traceability::{coverage_gaps, extract_spec_ids};
use crate::schema::validate_json;
use crate::state::file_lock::lock_sdd;
use crate::state::state_store::TASK_STATUS_DONE;
use crate::state::StateStore;

pub fn run_verify(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    let timeout_ms = args
        .and_then(|a| a.get("timeout"))
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0) as u64);
    let _guard = lock_sdd(cwd, "sdd verify", None, timeout_ms)?;

    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    let business_cwd = state
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.worktree_path.clone())
        .unwrap_or_else(|| cwd.to_string());
    let change_id = current_change_id(&state)?;
    let change_dir = PathBuf::from(cwd).join(".sdd/changes").join(&change_id);
    for key in [
        format!("{change_id}:spec"),
        format!("{change_id}:design"),
        format!("{change_id}:plan"),
        format!("{change_id}:plan-md"),
        format!("{change_id}:tasks-md"),
    ] {
        crate::state::artifact_store::verify_artifact(cwd, &key)?;
    }

    let spec = crate::state::runtime_store::read_change_field(cwd, &change_id, "spec")?
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 spec"))?;
    let tasks = read_plan_tasks(cwd, &change_id)?;
    // DONE 判定只信 state.tasks（运行权威）：plan 里的 status 只是初始 PENDING 声明，
    // 任务是否完成以运行时状态为准
    let done_ids: HashSet<String> = tasks
        .iter()
        .filter(|task| {
            state
                .tasks
                .get(&task.id)
                .map(|status| status == TASK_STATUS_DONE)
                .unwrap_or(false)
        })
        .map(|task| task.id.clone())
        .collect();
    let (requirement_ids, scenario_ids) = extract_spec_ids(&spec);
    let gaps = coverage_gaps(&requirement_ids, &scenario_ids, &tasks, &done_ids);

    let mut issues: Vec<Issue> = gaps
        .iter()
        .map(|gap| Issue {
            code: "E_VERIFY_REQUIRED".to_string(),
            severity: "high".to_string(),
            message: gap.clone(),
            file: None,
            category: None,
            start_line: None,
            end_line: None,
            existing_code: None,
            suggestion_code: None,
            origin: None,
        })
        .collect();
    if issues.is_empty() && tasks.is_empty() {
        issues.push(Issue {
            code: "E_VERIFY_REQUIRED".to_string(),
            severity: "high".to_string(),
            message: "没有可验证的任务（请先执行 sdd plan 与 sdd build）".to_string(),
            file: None,
            category: None,
            start_line: None,
            end_line: None,
            existing_code: None,
            suggestion_code: None,
            origin: None,
        });
    }
    if let Some(run_id) = &state.current_run_id {
        let results = crate::state::runtime_store::read_run_field(cwd, run_id, "tasks")?
            .unwrap_or_else(|| json!({}));
        for task in &tasks {
            let result = results.get(&task.id).cloned();
            match result {
                Some(result) => {
                    let valid = crate::protocol::validate_task_result(&result)
                        .and_then(|parsed| {
                            crate::commands::build::validate_task_evidence(task, &parsed)
                        })
                        .is_ok()
                        && result.get("status").and_then(|value| value.as_str())
                            == Some("completed");
                    if valid {
                        continue;
                    }
                    issues.push(Issue {
                        code: "E_TDD_EVIDENCE_REQUIRED".to_string(),
                        severity: "high".to_string(),
                        message: format!("任务 {} 缺少有效的完成证据", task.id),
                        file: Some(format!("runtime://runs/{run_id}/tasks/{}", task.id)),
                        category: None,
                        start_line: None,
                        end_line: None,
                        existing_code: None,
                        suggestion_code: None,
                        origin: None,
                    });
                }
                None => issues.push(Issue {
                    code: "E_TDD_EVIDENCE_REQUIRED".to_string(),
                    severity: "high".to_string(),
                    message: format!("任务 {} 缺少有效的完成结果", task.id),
                    file: Some(format!("runtime://runs/{run_id}/tasks/{}", task.id)),
                    category: None,
                    start_line: None,
                    end_line: None,
                    existing_code: None,
                    suggestion_code: None,
                    origin: None,
                }),
            }
        }
    } else {
        issues.push(Issue {
            code: "E_TDD_EVIDENCE_REQUIRED".to_string(),
            severity: "high".to_string(),
            message: "状态缺少 currentRunId，无法验证任务结果".to_string(),
            file: None,
            category: None,
            start_line: None,
            end_line: None,
            existing_code: None,
            suggestion_code: None,
            origin: None,
        });
    }

    let passed = issues.is_empty();
    let mut report = Report::new("verify", Some(change_id.clone()));
    report.passed = passed;
    report.summary = if passed {
        format!(
            "规格、任务与证据覆盖完整（{} 个需求，{} 个场景）",
            requirement_ids.len(),
            scenario_ids.len()
        )
    } else {
        format!("发现 {} 个覆盖缺口", issues.len())
    };
    report.issues = issues;
    if GitInspector::is_git_repo(&business_cwd) {
        report.minimality = Some(json!({
            "gitFingerprint": GitInspector::workspace_fingerprint(&business_cwd)?,
        }));
    }
    let report_value = serde_json::to_value(&report)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化报告失败：{e}")))?;
    validate_json("report", &report_value)?;
    let report_text = serde_json::to_string_pretty(&report_value)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("格式化报告失败：{e}")))?;
    fs::write(
        change_dir.join("verify-report.md"),
        render_report_markdown(&report),
    )
    .map_err(|e| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("写入 verify-report.md 失败：{e}"),
        )
    })?;
    let mut reports = crate::state::runtime_store::read_change_field(cwd, &change_id, "reports")?
        .unwrap_or_else(|| json!({}));
    reports["verify"] = report_value;
    crate::state::runtime_store::write_change_field(cwd, &change_id, "reports", reports)?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:verify-report"),
        "report",
        &format!("runtime://changes/{change_id}/reports/verify"),
        &report_text,
        json!({ "taskCount": tasks.len() }),
    )?;

    if !passed {
        store.update(|s| {
            s.current_phase = "BUILD_READY".to_string();
            s.failed_command = Some("sdd verify".to_string());
            s.failed_reason = Some(report.summary.clone());
            s.suggested_command = Some("sdd verify".to_string());
            s.last_command = Some("sdd verify".to_string());
        })?;
        return Err(SddError::new("E_VERIFY_REQUIRED", &report.summary).with_next("sdd verify"));
    }

    store.update(|s| {
        s.current_phase = "VERIFY_READY".to_string();
        s.in_progress_phase = None;
        s.suggested_command = Some("sdd review".to_string());
        s.last_command = Some("sdd verify".to_string());
        s.last_error = None;
    })?;

    Ok(CommandResult {
        ok: true,
        state: "VERIFY_READY".to_string(),
        exit_code: 0,
        change_id: Some(change_id),
        next: Some("sdd review".to_string()),
        data: Some(json!({ "report": report })),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}
