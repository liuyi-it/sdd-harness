//! review 命令：确定性审查、范围检查与敏感信息扫描。
//!
//! 翻译自 早期 Node 实现 + quality/deterministic-review.ts：
//! - 敏感信息扫描（E_SECURITY_BLOCKED）
//! - 变更文件范围/数量指标（记录，不阻断）
//! - 写 report(kind=review)，状态推进 REVIEW_READY

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::commands::plan::read_plan_tasks;
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::git::GitInspector;
use crate::quality::report::{Issue, Report};
use crate::schema::validate_json;
use crate::security::secrets_scanner::scan_secrets;
use crate::security::task_scope::validate_file_change;
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
    let business_cwd = state
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.worktree_path.clone())
        .unwrap_or_else(|| cwd.to_string());
    let change_id = current_change_id(&state)?;
    let change_dir = PathBuf::from(cwd).join(".sdd/changes").join(&change_id);

    let mut issues: Vec<Issue> = Vec::new();
    let mut debt_files = Vec::new();

    for key in [
        format!("{change_id}:plan"),
        format!("{change_id}:plan-md"),
        format!("{change_id}:tasks-md"),
        format!("{change_id}:verify-report"),
    ] {
        crate::state::artifact_store::verify_artifact(cwd, &key)?;
    }
    let verify_report: Report = serde_json::from_str(
        &fs::read_to_string(change_dir.join("verify-report.json"))
            .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取验证报告失败：{e}")))?,
    )
    .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("验证报告解析失败：{e}")))?;
    if GitInspector::is_git_repo(&business_cwd) {
        let expected = verify_report
            .minimality
            .as_ref()
            .and_then(|value| value.get("gitFingerprint"))
            .and_then(|value| value.as_str());
        let current = GitInspector::workspace_fingerprint(&business_cwd)?;
        if expected != Some(current.as_str()) {
            issues.push(Issue {
                code: "E_VERIFY_REQUIRED".to_string(),
                severity: "high".to_string(),
                message: "验证后工作区发生变化，请重新执行 sdd verify".to_string(),
                file: None,
            });
        }
    }

    // 1. 变更文件扫描（git 可用时）
    let mut changed_files: Vec<String> = Vec::new();
    if GitInspector::is_git_repo(&business_cwd) {
        changed_files = if let Some(workspace) = &state.workspace {
            GitInspector::changes_since(
                &business_cwd,
                &workspace.baseline_changed_files,
                &workspace.baseline_file_hashes,
            )?
        } else {
            GitInspector::business_changes(&business_cwd)?
        };
    }

    let tasks = read_plan_tasks(cwd, &change_id)?;
    let allowed: Vec<String> = tasks
        .iter()
        .flat_map(|task| task.allowed_files.iter().cloned())
        .collect();
    let forbidden: Vec<String> = tasks
        .iter()
        .flat_map(|task| task.forbidden_files.iter().cloned())
        .collect();
    if let Err(error) = validate_file_change(&changed_files, &allowed, &[], &forbidden) {
        issues.push(Issue {
            code: error.code,
            severity: "critical".to_string(),
            message: error.message,
            file: None,
        });
    }

    let added_dependencies = if changed_files.iter().any(|path| path == "Cargo.toml") {
        let current =
            fs::read_to_string(PathBuf::from(&business_cwd).join("Cargo.toml")).map_err(|e| {
                SddError::new("E_PATH_OUTSIDE_REPO", &format!("读取 Cargo.toml 失败：{e}"))
            })?;
        let baseline = state
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.baseline_cargo_manifest.clone())
            .or(GitInspector::file_at_head(&business_cwd, "Cargo.toml")?)
            .unwrap_or_default();
        let before = cargo_dependency_names(&baseline);
        cargo_dependency_names(&current)
            .difference(&before)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if !added_dependencies.is_empty() {
        let plan_raw = fs::read_to_string(change_dir.join("plan.json")).map_err(|e| {
            SddError::new("E_MISSING_ARTIFACT", &format!("读取 plan.json 失败：{e}"))
        })?;
        let plan: serde_json::Value = serde_json::from_str(&plan_raw)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("plan.json 解析失败：{e}")))?;
        let declared = planned_dependency_additions(&plan);
        let unplanned: Vec<String> = added_dependencies
            .iter()
            .filter(|name| !declared.contains(name.as_str()))
            .cloned()
            .collect();
        if !unplanned.is_empty() {
            issues.push(Issue {
                code: "E_UNPLANNED_DEPENDENCY".to_string(),
                severity: "high".to_string(),
                message: format!("新增依赖未在 plan.json 中声明：{}", unplanned.join("、")),
                file: Some("Cargo.toml".to_string()),
            });
        }
    }
    // 2. 敏感信息扫描：读取变更文件内容检查
    for file in &changed_files {
        let path = PathBuf::from(&business_cwd).join(file);
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
            if content.contains("sdd-debt") || content.contains("ponytail:") {
                debt_files.push(file.clone());
                issues.push(Issue {
                    code: "W_DEBT_MARKER".to_string(),
                    severity: "low".to_string(),
                    message: format!("文件 {file} 包含显式债务标记"),
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
    report.passed = !issues
        .iter()
        .any(|issue| issue.severity == "critical" || issue.severity == "high");
    report.summary = if issues.is_empty() {
        "未发现阻断问题".to_string()
    } else {
        format!("发现 {} 个审查发现", issues.len())
    };
    report.issues = issues.clone();
    let git_fingerprint = if GitInspector::is_git_repo(&business_cwd) {
        Some(GitInspector::workspace_fingerprint(&business_cwd)?)
    } else {
        None
    };
    report.minimality = Some(json!({
        "fileCount": changed_count,
        "addedDependencies": added_dependencies,
        "debtFiles": debt_files,
        "gitFingerprint": git_fingerprint,
    }));
    let report_value = serde_json::to_value(&report)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化报告失败：{e}")))?;
    validate_json("report", &report_value)?;

    // 写报告
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化报告失败：{e}")))?;
    fs::write(change_dir.join("review-report.json"), &report_text)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入报告失败：{e}")))?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:review-report"),
        "report",
        &format!(".sdd/changes/{change_id}/review-report.json"),
        &report_text,
        json!({ "changedFiles": changed_files }),
    )?;

    // 阻断性发现（critical）→ E_SECURITY_BLOCKED
    if !report.passed {
        let requires_verify = issues.iter().any(|issue| issue.code == "E_VERIFY_REQUIRED");
        let (phase, next) = if requires_verify {
            ("BUILD_READY", "sdd verify")
        } else {
            ("VERIFY_READY", "sdd review")
        };
        store.update(|s| {
            s.current_phase = phase.to_string();
            s.failed_command = Some("sdd review".to_string());
            s.failed_reason = Some(report.summary.clone());
            s.suggested_command = Some(next.to_string());
            s.last_command = Some("sdd review".to_string());
        })?;
        let code = issues
            .iter()
            .find(|issue| issue.severity == "critical")
            .or_else(|| issues.iter().find(|issue| issue.severity == "high"))
            .map(|issue| issue.code.as_str())
            .unwrap_or("E_REVIEW_FAILED");
        return Err(SddError::new(code, "审查发现阻断性问题").with_next(next));
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

fn cargo_dependency_names(content: &str) -> BTreeSet<String> {
    let mut section = "";
    let mut dependencies = BTreeSet::new();
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            section = line.trim_matches(['[', ']']).trim();
            if dependency_subtable(section) {
                dependencies.insert(section.rsplit('.').next().unwrap_or(section).to_string());
            }
            continue;
        }
        if dependency_section(section) {
            if let Some((name, _)) = line.split_once('=') {
                let name = name.trim().trim_matches(['\'', '"']);
                if !name.is_empty() && !name.starts_with('#') {
                    dependencies.insert(name.to_string());
                }
            }
        }
    }
    dependencies
}

fn dependency_section(section: &str) -> bool {
    matches!(
        section,
        "dependencies" | "dev-dependencies" | "build-dependencies"
    ) || section.ends_with(".dependencies")
        || section.ends_with(".dev-dependencies")
        || section.ends_with(".build-dependencies")
}

fn dependency_subtable(section: &str) -> bool {
    section.starts_with("dependencies.")
        || section.starts_with("dev-dependencies.")
        || section.starts_with("build-dependencies.")
        || section.contains(".dependencies.")
        || section.contains(".dev-dependencies.")
        || section.contains(".build-dependencies.")
}

fn planned_dependency_additions(plan: &serde_json::Value) -> BTreeSet<String> {
    plan.get("dependencies")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter(|item| item.get("action").and_then(|value| value.as_str()) == Some("ADD"))
        .filter_map(|item| item.get("name").and_then(|value| value.as_str()))
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{cargo_dependency_names, planned_dependency_additions};

    #[test]
    fn cargo_dependencies_ignore_metadata_and_detect_target_tables() {
        let names = cargo_dependency_names(
            r#"
[package]
name = "demo"
[dependencies]
serde = "1"
[target.'cfg(unix)'.dev-dependencies]
tempfile = "3"
[dependencies.regex]
version = "1"
"#,
        );
        assert_eq!(
            names.into_iter().collect::<Vec<_>>(),
            ["regex", "serde", "tempfile"]
        );
    }

    #[test]
    fn only_add_decisions_authorize_new_dependencies() {
        let plan = serde_json::json!({
            "dependencies": [
                { "name": "serde", "action": "ADD" },
                { "name": "regex", "action": "UPDATE" }
            ]
        });
        assert_eq!(
            planned_dependency_additions(&plan)
                .into_iter()
                .collect::<Vec<_>>(),
            ["serde"]
        );
    }
}
