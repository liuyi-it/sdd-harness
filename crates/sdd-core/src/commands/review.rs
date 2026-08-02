//! review 命令：确定性审查、范围检查与敏感信息扫描。
//!
//! 翻译自 Node 版 `packages/core/src/commands/review.ts` + quality/deterministic-review.ts：
//! - 敏感信息扫描（E_SECURITY_BLOCKED）
//! - 变更文件范围/数量指标（记录，不阻断）
//! - 写 report(kind=review)，状态推进 REVIEW_READY

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::git::GitInspector;
use crate::quality::report::{Issue, Report};
use crate::security::secrets_scanner::scan_secrets;
use crate::state::file_lock::lock_sdd;
use crate::state::StateStore;

pub fn run_review(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    let timeout_ms = args
        .and_then(|a| a.get("timeout"))
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0) as u64);
    let _guard = lock_sdd(cwd, "sdd review", None, timeout_ms)?;

    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    let change_id = current_change_id(&state)?;
    let change_dir = PathBuf::from(cwd).join(".sdd/changes").join(&change_id);

    let mut issues: Vec<Issue> = Vec::new();

    // 1. 变更文件扫描（git 可用时）
    let mut changed_files: Vec<String> = Vec::new();
    if GitInspector::is_git_repo(cwd) {
        if let Ok(files) = GitInspector::changed_files(cwd) {
            changed_files = files;
        }
    }
    // 2. 敏感信息扫描：读取变更文件内容检查
    for file in &changed_files {
        let path = PathBuf::from(cwd).join(file);
        if let Ok(content) = fs::read_to_string(&path) {
            let hits = scan_secrets(&content);
            if !hits.is_empty() {
                let names: Vec<String> = hits.iter().map(|(n, _)| n.clone()).collect();
                issues.push(Issue {
                    code: "E_SECURITY_BLOCKED".to_string(),
                    severity: "critical".to_string(),
                    message: format!("文件 {file} 包含敏感信息（{}）", names.join("、")),
                    file: Some(file.clone()),
                });
            }
        }
    }

    // 3. 变更规模指标（记录不阻断）
    let changed_count = changed_files.len();
    if changed_count > 0 {
        issues.push(Issue {
            code: "W_CHANGE_SIZE".to_string(),
            severity: "low".to_string(),
            message: format!("检测到 {} 个变更文件", changed_count),
            file: None,
        });
    }

    let mut report = Report::new("review", Some(change_id.clone()));
    report.passed = true;
    report.summary = if issues.is_empty() {
        "未发现阻断问题".to_string()
    } else {
        format!("发现 {} 个审查发现", issues.len())
    };
    report.issues = issues.clone();

    // 写报告
    fs::write(
        change_dir.join("review-report.json"),
        serde_json::to_string_pretty(&report)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化报告失败：{e}")))?,
    )
    .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入报告失败：{e}")))?;

    // 阻断性发现（critical）→ E_SECURITY_BLOCKED
    if issues.iter().any(|i| i.severity == "critical") {
        store.update(|s| {
            s.current_phase = "REVIEW_READY".to_string();
            s.failed_command = Some("sdd review".to_string());
            s.failed_reason = Some(report.summary.clone());
            s.suggested_command = Some("sdd build complete".to_string());
            s.last_command = Some("sdd review".to_string());
        })?;
        return Err(
            SddError::new("E_SECURITY_BLOCKED", "审查发现敏感信息或阻断性问题")
                .with_next("sdd status"),
        );
    }

    store.update(|s| {
        s.current_phase = "REVIEW_READY".to_string();
        s.in_progress_phase = None;
        s.suggested_command = Some("sdd archive".to_string());
        s.last_command = Some("sdd review".to_string());
        s.last_error = None;
    })?;

    Ok(CommandResult {
        ok: true,
        state: "REVIEW_READY".to_string(),
        exit_code: 0,
        change_id: Some(change_id),
        next: Some("sdd archive".to_string()),
        data: Some(json!({ "report": report })),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}
