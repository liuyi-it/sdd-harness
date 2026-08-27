//! review 命令：确定性审查、范围检查与敏感信息扫描。
//!
//! 审查链包含：
//! - 敏感信息扫描（E_SECURITY_BLOCKED）
//! - 变更文件范围/数量指标（记录，不阻断）
//! - 写 report(kind=review)，状态推进 REVIEW_READY

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::commands::plan::plan_tasks;
use crate::contracts::{CliWarning, CommandResult};
use crate::error::SddError;
use crate::git::inspector::RepoEntryContent;
use crate::git::GitInspector;
use crate::quality::ocr::{
    validate_output, OcrComment, OcrConfig, OcrExecution, OcrExecutor, OcrMode, OcrOutput,
    SystemOcrExecutor,
};
use crate::quality::report::{render_report_markdown, Issue, Report};
use crate::schema::validate_json;
use crate::security::secrets_scanner::validate_no_secrets;
use crate::security::task_scope::validate_file_change;
use crate::state::artifact_store::ArtifactRecord;
use crate::state::file_lock::lock_initialized_sdd;

pub fn run_review(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "changeId"])?;
    let timeout_ms = super::timeout_ms(args)?;
    let _guard = lock_initialized_sdd(cwd, "sdd review", None, timeout_ms)?;
    run_review_with_executor(cwd, args, &SystemOcrExecutor)
}

fn run_review_with_executor<E: OcrExecutor>(
    cwd: &str,
    args: Option<&serde_json::Value>,
    executor: &E,
) -> Result<CommandResult, SddError> {
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let state = runtime.state.clone();
    super::ensure_phase(cwd, &state, "review", args)?;
    let business_cwd = state
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.worktree_path.clone())
        .unwrap_or_else(|| cwd.to_string());
    let change_id = current_change_id(&state)?;
    let change_dir = crate::state::paths::change_dir(cwd, &change_id, false)?;
    let config = &runtime.config;

    let mut issues: Vec<Issue> = Vec::new();
    let mut debt_files = Vec::new();
    let mut warnings = Vec::new();
    let is_git = GitInspector::is_git_repo(&business_cwd)?;
    let workspace =
        if is_git {
            Some(state.workspace.as_ref().ok_or_else(|| {
                SddError::new("E_STATE_CORRUPTED", "Git 工作流缺少基线 workspace")
            })?)
        } else {
            None
        };

    crate::state::artifact_store::verify_artifacts_in(
        cwd,
        &runtime,
        [
            format!("{change_id}:plan"),
            format!("{change_id}:plan-md"),
            format!("{change_id}:tasks-md"),
            format!("{change_id}:verify-report"),
        ],
    )?;
    let change = runtime
        .changes
        .get(&change_id)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少当前变更"))?;
    let verify_report: Report = change
        .get("reports")
        .and_then(|reports| reports.get("verify"))
        .cloned()
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 verify 报告"))
        .and_then(|value| {
            serde_json::from_value(value)
                .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("验证报告解析失败：{e}")))
        })?;
    if is_git {
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
                category: None,
                start_line: None,
                end_line: None,
                existing_code: None,
                suggestion_code: None,
                origin: None,
            });
        }
    } else {
        // 非 git 仓库：跳过 git 类扫描时明确提示，避免静默缺少事实校验
        warnings.push(CliWarning::new(
            "W_NO_GIT_SCOPE_CHECK",
            "当前目录不是 git 仓库，未执行 git 事实校验（文件范围、依赖增量、工作区指纹）",
        ));
    }

    // 1. 变更文件扫描（git 可用时）
    let mut changed_files: Vec<String> = Vec::new();
    if let Some(workspace) = workspace {
        changed_files = GitInspector::changes_since(
            &business_cwd,
            &workspace.baseline_changed_files,
            &workspace.baseline_file_hashes,
        )?;
    }

    let plan = change
        .get("plan")
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 plan"))?;
    let tasks = plan_tasks(plan)?;
    crate::commands::plan::validate_dependencies(plan.get("dependencies").ok_or_else(|| {
        SddError::new(
            "E_STATE_CORRUPTED",
            "runtime.json 的 plan 缺少 dependencies",
        )
    })?)
    .map_err(|error| SddError::new("E_STATE_CORRUPTED", &error.message))?;
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
            category: None,
            start_line: None,
            end_line: None,
            existing_code: None,
            suggestion_code: None,
            origin: None,
        });
    }

    let added_dependencies = if changed_files.iter().any(|path| path == "Cargo.toml") {
        let manifest = GitInspector::resolve_repo_path(&business_cwd, "Cargo.toml")?;
        let current = match fs::read_to_string(manifest) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(SddError::new(
                    "E_PATH_OUTSIDE_REPO",
                    &format!("读取 Cargo.toml 失败：{error}"),
                ));
            }
        };
        let baseline = workspace
            .expect("Git 工作流已验证 workspace")
            .baseline_cargo_manifest
            .as_deref()
            .unwrap_or("");
        let before = cargo_dependency_names(baseline);
        cargo_dependency_names(&current)
            .difference(&before)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if !added_dependencies.is_empty() {
        let declared = planned_dependency_additions(plan);
        let unplanned: Vec<String> = added_dependencies
            .iter()
            .filter(|name| !declared.contains(name.as_str()))
            .cloned()
            .collect();
        if !unplanned.is_empty() {
            issues.push(Issue {
                code: "E_UNPLANNED_DEPENDENCY".to_string(),
                severity: "high".to_string(),
                message: format!(
                    "新增依赖未在 runtime.json 的 plan 中声明：{}",
                    unplanned.join("、")
                ),
                file: Some("Cargo.toml".to_string()),
                category: None,
                start_line: None,
                end_line: None,
                existing_code: None,
                suggestion_code: None,
                origin: None,
            });
        }
    }
    // 2. 敏感信息扫描：读取变更文件内容检查。
    //    路径先经 resolve_repo_path 解析（防 symlink 逃逸，与 ocr.rs 一致）；
    //    扫描受 config audit.maxSizeMb/maxFiles 限额约束（默认 5MB/200 文件）。
    let (audit_max_files, audit_max_bytes) = audit_limits(config)?;
    let mut scanned_count = 0usize;
    let mut scanned_bytes = 0usize;
    let mut scan_limited = false;
    let mut scanned_line_counts = BTreeMap::new();
    for file in &changed_files {
        if scanned_count >= audit_max_files || scanned_bytes >= audit_max_bytes {
            scan_limited = true;
            break;
        }
        let remaining = audit_max_bytes - scanned_bytes;
        match GitInspector::read_entry_with_limit(&business_cwd, file, remaining)? {
            RepoEntryContent::Content(bytes) => {
                scanned_count += 1;
                scanned_bytes += bytes.len();
                if let Ok(content) = std::str::from_utf8(&bytes) {
                    scanned_line_counts.insert(file.clone(), content.lines().count());
                }
                let content = String::from_utf8_lossy(&bytes);
                if let Err(error) = validate_no_secrets([(file.as_str(), content.as_ref())]) {
                    issues.push(Issue {
                        code: error.code,
                        severity: "critical".to_string(),
                        message: error.message,
                        file: Some(file.clone()),
                        category: None,
                        start_line: None,
                        end_line: None,
                        existing_code: None,
                        suggestion_code: None,
                        origin: None,
                    });
                }
                if content.contains("sdd-debt") || content.contains("ponytail:") {
                    debt_files.push(file.clone());
                    issues.push(Issue {
                        code: "W_DEBT_MARKER".to_string(),
                        severity: "low".to_string(),
                        message: format!("文件 {file} 包含显式债务标记"),
                        file: Some(file.clone()),
                        category: None,
                        start_line: None,
                        end_line: None,
                        existing_code: None,
                        suggestion_code: None,
                        origin: None,
                    });
                }
            }
            RepoEntryContent::Missing => {
                // 删除文件没有内容可扫描，但仍计入已处理文件范围。
                scanned_count += 1;
            }
            RepoEntryContent::TooLarge => {
                scan_limited = true;
                break;
            }
        }
    }
    if scan_limited {
        issues.push(Issue {
            code: "E_AUDIT_SCAN_INCOMPLETE".to_string(),
            severity: "critical".to_string(),
            message: format!(
                "变更文件扫描达到配置上限（{} 个文件 / {} MB），无法确认未扫描文件不含敏感信息",
                audit_max_files,
                audit_max_bytes / (1024 * 1024)
            ),
            file: None,
            category: None,
            start_line: None,
            end_line: None,
            existing_code: None,
            suggestion_code: None,
            origin: None,
        });
    }

    // 3. 变更规模指标（记录不阻断）
    let changed_count = changed_files.len();
    if changed_count > 0 {
        issues.push(Issue {
            code: "W_CHANGE_SIZE".to_string(),
            severity: "low".to_string(),
            message: format!("检测到 {} 个变更文件", changed_count),
            file: None,
            category: None,
            start_line: None,
            end_line: None,
            existing_code: None,
            suggestion_code: None,
            origin: None,
        });
    }

    let mut report = Report::new("review", change_id.clone());
    report.passed = !issues
        .iter()
        .any(|issue| issue.severity == "critical" || issue.severity == "high");
    report.summary = if issues.is_empty() {
        "未发现阻断问题".to_string()
    } else {
        format!("发现 {} 个审查发现", issues.len())
    };
    report.issues = issues.clone();
    let git_fingerprint = if is_git {
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
    let mut ocr_error = None;
    if !report.passed {
        set_ocr_status(
            &mut report,
            json!({ "status": "blocked-by-deterministic-review" }),
        );
    } else if changed_files.is_empty() {
        set_ocr_status(
            &mut report,
            json!({ "status": "skipped", "reason": "no-changes" }),
        );
    } else {
        let ocr_config = OcrConfig::from_config(config)?;
        match ocr_config.mode {
            OcrMode::Off => {
                set_ocr_status(&mut report, json!({ "status": "off" }));
            }
            OcrMode::Auto | OcrMode::Required => {
                let timeout = ocr_timeout(args)?;
                let changed_set = changed_files.iter().cloned().collect::<BTreeSet<_>>();
                match executor.execute(
                    std::path::Path::new(&business_cwd),
                    &ocr_config.command,
                    timeout,
                ) {
                    Ok(OcrExecution::NotFound) if ocr_config.mode == OcrMode::Auto => {
                        warnings.push(CliWarning::new(
                            "W_OCR_NOT_FOUND",
                            "未找到 OCR 命令，已返回确定性审查结果",
                        ));
                        set_ocr_status(
                            &mut report,
                            json!({
                                "status": "not-found",
                                "backend": "deterministic",
                            }),
                        );
                    }
                    Ok(OcrExecution::NotFound) => {
                        let error =
                            SddError::new("E_REVIEW_BACKEND_UNAVAILABLE", "未找到 OCR 命令")
                                .with_next("sdd review");
                        set_ocr_status(&mut report, json!({ "status": "unavailable" }));
                        record_ocr_error(&mut report, &mut issues, &error);
                        ocr_error = Some(error);
                    }
                    Ok(OcrExecution::Completed(output)) => {
                        match validate_output(*output, &changed_set, &scanned_line_counts) {
                            Ok(output) => {
                                let metadata = ocr_metadata(&output);
                                for comment in &output.comments {
                                    issues.push(merge_ocr_comment(comment));
                                }
                                set_ocr_status(&mut report, metadata);
                            }
                            Err(error) => {
                                let status = if error.code == "E_REVIEW_BACKEND_FAILED" {
                                    "failed"
                                } else {
                                    "invalid-output"
                                };
                                set_ocr_status(&mut report, json!({ "status": status }));
                                record_ocr_error(&mut report, &mut issues, &error);
                                ocr_error = Some(error);
                            }
                        }
                    }
                    Err(error) => {
                        set_ocr_status(&mut report, json!({ "status": "failed" }));
                        record_ocr_error(&mut report, &mut issues, &error);
                        ocr_error = Some(error);
                    }
                }
            }
        }
    }
    report.issues = issues.clone();
    report.passed = !issues
        .iter()
        .any(|issue| issue.severity == "critical" || issue.severity == "high");
    report.summary = if issues.is_empty() {
        "未发现阻断问题".to_string()
    } else {
        format!("发现 {} 个审查发现", issues.len())
    };

    let report_value = serde_json::to_value(&report)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化报告失败：{e}")))?;
    validate_json("report", &report_value)?;
    let report_markdown = render_report_markdown(&report);
    crate::safe_fs::atomic_write(
        &change_dir.join("review-report.md"),
        report_markdown.as_bytes(),
        "review-report.md",
    )?;
    let requires_verify = issues.iter().any(|issue| issue.code == "E_VERIFY_REQUIRED");
    let (phase, next) = if report.passed {
        ("REVIEW_READY", "sdd archive")
    } else if requires_verify {
        ("BUILD_READY", "sdd verify")
    } else {
        ("VERIFY_READY", "sdd review")
    };
    let artifact_key = format!("{change_id}:review-report");
    let content_path = format!("runtime://changes/{change_id}/reports/review");
    let report_summary = report.summary.clone();
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        let reports = super::reports_mut(super::change_mut(document, &change_id)?)?;
        reports.insert("review".to_string(), report_value);
        crate::state::artifact_store::record_artifacts_in(
            cwd,
            document,
            vec![ArtifactRecord {
                key: &artifact_key,
                artifact_type: "report",
                content_path: &content_path,
                inputs: json!({ "changedFiles": &changed_files }),
            }],
        )?;
        crate::state::state_store::apply_state_update(&mut document.state, |state| {
            state.current_phase = phase.to_string();
            state.suggested_command = Some(next.to_string());
            state.last_command = Some("sdd review".to_string());
            if report.passed {
                state.in_progress_phase = None;
                state.clear_failure();
            } else {
                state.record_failure("sdd review", report_summary);
            }
        })?;
        Ok(())
    })?;

    // 阻断性发现（critical）→ E_SECURITY_BLOCKED
    if !report.passed {
        if let Some(error) = ocr_error {
            return Err(error.with_next(next));
        }
        let code = issues
            .iter()
            .find(|issue| issue.severity == "critical")
            .or_else(|| issues.iter().find(|issue| issue.severity == "high"))
            .map(|issue| match issue.code.as_str() {
                "OCR_FINDING" => "E_REVIEW_FAILED",
                code => code,
            })
            .unwrap_or("E_REVIEW_FAILED");
        return Err(SddError::new(code, "审查发现阻断性问题").with_next(next));
    }

    Ok(CommandResult {
        ok: true,
        state: "REVIEW_READY".to_string(),
        exit_code: 0,
        change_id: Some(change_id),
        next: Some("sdd archive".to_string()),
        data: Some(json!({ "report": report })),
        rendered: None,
        warnings: if warnings.is_empty() {
            None
        } else {
            Some(warnings)
        },
        action_required: None,
        error: None,
    })
}

fn ocr_timeout(args: Option<&serde_json::Value>) -> Result<std::time::Duration, SddError> {
    Ok(super::timeout_ms(args)?
        .map(std::time::Duration::from_millis)
        .unwrap_or_else(|| std::time::Duration::from_secs(120)))
}

/// 读取当前 config 中的审计扫描限额。
fn audit_limits(config: &serde_json::Value) -> Result<(usize, usize), SddError> {
    let max_files = config
        .pointer("/audit/maxFiles")
        .and_then(|value| value.as_u64())
        .filter(|count| *count > 0)
        .and_then(|count| usize::try_from(count).ok())
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "audit.maxFiles 必须是正整数"))?;
    let max_bytes = config
        .pointer("/audit/maxSizeMb")
        .and_then(|value| value.as_u64())
        .filter(|mb| *mb > 0)
        .and_then(|mb| usize::try_from(mb).ok())
        .and_then(|mb| mb.checked_mul(1024 * 1024))
        .ok_or_else(|| {
            SddError::new("E_STATE_CORRUPTED", "audit.maxSizeMb 必须是可表示的正整数")
        })?;
    Ok((max_files, max_bytes))
}

fn set_ocr_status(report: &mut Report, status: serde_json::Value) {
    report
        .minimality
        .as_mut()
        .and_then(serde_json::Value::as_object_mut)
        .expect("review 在 OCR 阶段前已初始化 minimality")
        .insert("ocr".to_string(), status);
}

fn ocr_metadata(output: &OcrOutput) -> serde_json::Value {
    json!({
        "status": output.status,
        "runId": output.manifest.run_id,
        "provider": output.llm.provider,
        "model": output.llm.model,
        "filesReviewed": output.summary.files_reviewed,
        "commentCount": output.summary.comments,
        "totalTokens": output.summary.total_tokens,
        "toolCalls": output.tool_calls.total,
    })
}

fn merge_ocr_comment(comment: &OcrComment) -> Issue {
    Issue {
        code: "OCR_FINDING".to_string(),
        severity: comment.severity.clone(),
        message: comment.content.clone(),
        file: Some(comment.path.clone()),
        category: Some(comment.category.clone()),
        start_line: Some(comment.start_line),
        end_line: Some(comment.end_line),
        existing_code: comment.existing_code.clone(),
        suggestion_code: comment.suggestion_code.clone(),
        origin: Some("ocr".to_string()),
    }
}

fn record_ocr_error(report: &mut Report, issues: &mut Vec<Issue>, error: &SddError) {
    issues.push(Issue {
        code: error.code.clone(),
        severity: "critical".to_string(),
        message: format!("OCR 后端失败：{}", error.code),
        file: None,
        category: Some("other".to_string()),
        start_line: None,
        end_line: None,
        existing_code: None,
        suggestion_code: None,
        origin: Some("ocr".to_string()),
    });
    report.passed = false;
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
    plan["dependencies"]
        .as_array()
        .expect("validate_dependencies 已确认 dependencies 是数组")
        .iter()
        .filter(|item| item.get("action").and_then(|value| value.as_str()) == Some("ADD"))
        .map(|item| {
            item["name"]
                .as_str()
                .expect("validate_dependencies 已确认 name 是字符串")
        })
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{audit_limits, cargo_dependency_names, planned_dependency_additions};
    use crate::git::inspector::RepoEntryContent;
    use crate::git::GitInspector;

    #[test]
    fn audit_limits_require_current_positive_values() {
        let config = serde_json::json!({
            "audit": { "maxSizeMb": 2, "maxFiles": 50 }
        });
        let (files, bytes) = audit_limits(&config).unwrap();
        assert_eq!(files, 50);
        assert_eq!(bytes, 2 * 1024 * 1024);
        assert!(audit_limits(&serde_json::json!({})).is_err());
        assert!(audit_limits(&serde_json::json!({
            "audit": { "maxSizeMb": 1, "maxFiles": 0 }
        }))
        .is_err());
        assert!(audit_limits(&serde_json::json!({
            "audit": { "maxSizeMb": u64::MAX, "maxFiles": 1 }
        }))
        .is_err());
    }

    #[test]
    fn bounded_text_reader_skips_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.txt");
        std::fs::write(&path, "0123456789").unwrap();
        let cwd = dir.path().to_string_lossy();

        assert_eq!(
            GitInspector::read_entry_with_limit(&cwd, "large.txt", 9).unwrap(),
            RepoEntryContent::TooLarge
        );
        assert_eq!(
            GitInspector::read_entry_with_limit(&cwd, "large.txt", 10).unwrap(),
            RepoEntryContent::Content(b"0123456789".to_vec())
        );
    }

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
                {
                    "name": "serde", "manifest": "Cargo.toml", "action": "ADD",
                    "reason": "序列化", "requirements": ["REQ-001"]
                },
                {
                    "name": "regex", "manifest": "Cargo.toml", "action": "UPDATE",
                    "reason": "升级", "requirements": ["REQ-001"]
                }
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
