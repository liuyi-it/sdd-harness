//! verify 命令：验证规格、任务与证据覆盖。
//!
//! 翻译自 Node 版 `packages/core/src/commands/verify.ts`：
//! 检查 Requirement/Scenario 覆盖、任务完成状态与 TDD 证据，
//! 写 report(kind=verify)，状态推进 VERIFY_READY 或报 E_VERIFY_REQUIRED。

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::commands::plan::read_plan_tasks;
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::quality::report::{Issue, Report};
use crate::quality::traceability::{coverage_gaps, extract_spec_ids};
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
    let change_id = current_change_id(&state)?;
    let change_dir = PathBuf::from(cwd).join(".sdd/changes").join(&change_id);

    let spec_json_raw = fs::read_to_string(change_dir.join("spec.json"))
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 spec.json 失败：{e}")))?;
    let spec_json: serde_json::Value = serde_json::from_str(&spec_json_raw)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("spec.json 解析失败：{e}")))?;

    let tasks = read_plan_tasks(cwd, &change_id)?;
    let done_ids: HashSet<String> = tasks
        .iter()
        .filter(|t| {
            t.status == TASK_STATUS_DONE
                || state
                    .tasks
                    .get(&t.id)
                    .map(|s| s == TASK_STATUS_DONE)
                    .unwrap_or(false)
        })
        .map(|t| t.id.clone())
        .collect();

    let (requirement_ids, scenario_ids) = extract_spec_ids(&spec_json);
    let gaps = coverage_gaps(&requirement_ids, &scenario_ids, &tasks, &done_ids);

    let mut issues: Vec<Issue> = gaps
        .iter()
        .map(|g| Issue {
            code: "E_VERIFY_REQUIRED".to_string(),
            severity: "high".to_string(),
            message: g.clone(),
            file: None,
        })
        .collect();
    if issues.is_empty() && tasks.is_empty() {
        issues.push(Issue {
            code: "E_VERIFY_REQUIRED".to_string(),
            severity: "high".to_string(),
            message: "没有可验证的任务（请先执行 sdd plan 与 sdd build）".to_string(),
            file: None,
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

    // 写报告
    fs::write(
        change_dir.join("verify-report.json"),
        serde_json::to_string_pretty(&report)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化报告失败：{e}")))?,
    )
    .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入报告失败：{e}")))?;

    if !passed {
        store.update(|s| {
            s.current_phase = "VERIFY_READY".to_string();
            s.failed_command = Some("sdd verify".to_string());
            s.failed_reason = Some(report.summary.clone());
            s.suggested_command = Some("sdd build next".to_string());
            s.last_command = Some("sdd verify".to_string());
        })?;
        return Err(SddError::new("E_VERIFY_REQUIRED", &report.summary).with_next("sdd build next"));
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
