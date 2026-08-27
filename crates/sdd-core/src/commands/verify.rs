//! verify 命令：验证规格、任务与证据覆盖。
//!
//! 机器报告位于 `.sdd/runtime.json`，change 目录只保留 verify-report.md。

use std::collections::HashSet;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::commands::plan::plan_tasks;
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::git::GitInspector;
use crate::quality::report::{render_report_markdown, Issue, Report};
use crate::quality::traceability::{coverage_gaps, extract_spec_ids};
use crate::schema::validate_json;
use crate::state::artifact_store::ArtifactRecord;
use crate::state::file_lock::lock_initialized_sdd;
use crate::state::state_store::TASK_STATUS_DONE;

pub fn run_verify(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "changeId"])?;
    let timeout_ms = super::timeout_ms(args)?;
    let _guard = lock_initialized_sdd(cwd, "sdd verify", None, timeout_ms)?;

    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let state = runtime.state.clone();
    super::ensure_phase(cwd, &state, "verify", args)?;
    let business_cwd = state
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.worktree_path.clone())
        .unwrap_or_else(|| cwd.to_string());
    let change_id = current_change_id(&state)?;
    let change_dir = crate::state::paths::change_dir(cwd, &change_id, false)?;
    crate::state::artifact_store::verify_artifacts_in(
        cwd,
        &runtime,
        [
            format!("{change_id}:spec"),
            format!("{change_id}:design"),
            format!("{change_id}:plan"),
            format!("{change_id}:plan-md"),
            format!("{change_id}:tasks-md"),
        ],
    )?;

    let change = runtime
        .changes
        .get(&change_id)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少当前变更"))?;
    let spec = change
        .get("spec")
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 spec"))?;
    let specification = crate::engines::spec::model_from_record(spec)?;
    let plan = change
        .get("plan")
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 plan"))?;
    let tasks = plan_tasks(plan)?;
    crate::commands::build::validate_runtime_task_state(&state, &tasks)?;
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
    let (requirement_ids, scenario_ids) = extract_spec_ids(&specification);
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
    let run_id = state
        .current_run_id
        .as_deref()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "BUILD_READY 状态缺少 currentRunId"))?;
    let results = runtime
        .runs
        .get(run_id)
        .and_then(|run| run.get("tasks"))
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            SddError::new("E_STATE_CORRUPTED", "runtime.json 的运行任务结果必须是对象")
        })?;
    for task in &tasks {
        let result = results.get(&task.id).cloned();
        match result {
            Some(result) => {
                let valid = crate::protocol::validate_task_result(&result)
                    .and_then(|parsed| {
                        crate::commands::build::validate_task_evidence(task, &parsed)
                    })
                    .is_ok()
                    && result.get("status").and_then(|value| value.as_str()) == Some("completed");
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

    let passed = issues.is_empty();
    let mut report = Report::new("verify", change_id.clone());
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
    if GitInspector::is_git_repo(&business_cwd)? {
        report.minimality = Some(json!({
            "gitFingerprint": GitInspector::workspace_fingerprint(&business_cwd)?,
        }));
    }
    let report_value = serde_json::to_value(&report)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化报告失败：{e}")))?;
    validate_json("report", &report_value)?;
    let report_markdown = render_report_markdown(&report);
    crate::safe_fs::atomic_write(
        &change_dir.join("verify-report.md"),
        report_markdown.as_bytes(),
        "verify-report.md",
    )?;
    let artifact_key = format!("{change_id}:verify-report");
    let content_path = format!("runtime://changes/{change_id}/reports/verify");
    let task_count = tasks.len();
    let report_summary = report.summary.clone();
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        let reports = super::reports_mut(super::change_mut(document, &change_id)?)?;
        reports.insert("verify".to_string(), report_value);
        crate::state::artifact_store::record_artifacts_in(
            cwd,
            document,
            vec![ArtifactRecord {
                key: &artifact_key,
                artifact_type: "report",
                content_path: &content_path,
                inputs: json!({ "taskCount": task_count }),
            }],
        )?;
        crate::state::state_store::apply_state_update(&mut document.state, |state| {
            if passed {
                state.current_phase = "VERIFY_READY".to_string();
                state.in_progress_phase = None;
                state.clear_failure();
                state.suggested_command = Some("sdd review".to_string());
            } else {
                state.current_phase = "BUILD_READY".to_string();
                state.record_failure("sdd verify", report_summary);
                state.suggested_command = Some("sdd verify".to_string());
            }
            state.last_command = Some("sdd verify".to_string());
        })?;
        Ok(())
    })?;

    if !passed {
        return Err(SddError::new("E_VERIFY_REQUIRED", &report.summary).with_next("sdd verify"));
    }

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
