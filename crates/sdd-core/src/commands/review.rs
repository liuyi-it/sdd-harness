//! review 命令：确定性审查、范围检查与敏感信息扫描。
//!
//! 翻译自 早期 Node 实现 + quality/deterministic-review.ts：
//! - 敏感信息扫描（E_SECURITY_BLOCKED）
//! - 变更文件范围/数量指标（记录，不阻断）
//! - 写 report(kind=review)，状态推进 REVIEW_READY

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::commands::plan::read_plan_tasks;
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::git::GitInspector;
use crate::quality::ocr::{
    validate_output, OcrComment, OcrConfig, OcrExecution, OcrExecutor, OcrMode, OcrOutput,
    SystemOcrExecutor,
};
use crate::quality::report::{render_report_markdown, Issue, Report};
use crate::schema::validate_json;
use crate::security::secrets_scanner::validate_no_secrets;
use crate::security::task_scope::validate_file_change;
use crate::state::file_lock::lock_sdd;
use crate::state::StateStore;

pub fn run_review(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    let timeout_ms = args
        .and_then(|a| a.get("timeout"))
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0) as u64);
    let _guard = lock_sdd(cwd, "sdd review", None, timeout_ms)?;
    run_review_with_executor(cwd, args, &SystemOcrExecutor)
}

fn run_review_with_executor<E: OcrExecutor>(
    cwd: &str,
    args: Option<&serde_json::Value>,
    executor: &E,
) -> Result<CommandResult, SddError> {
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
    let mut warnings: Vec<serde_json::Value> = Vec::new();
    let is_git = GitInspector::is_git_repo(&business_cwd);

    for key in [
        format!("{change_id}:plan"),
        format!("{change_id}:plan-md"),
        format!("{change_id}:tasks-md"),
        format!("{change_id}:verify-report"),
    ] {
        crate::state::artifact_store::verify_artifact(cwd, &key)?;
    }
    let verify_report: Report =
        crate::state::runtime_store::read_change_field(cwd, &change_id, "reports")?
            .and_then(|reports| reports.get("verify").cloned())
            .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 verify 报告"))
            .and_then(|value| {
                serde_json::from_value(value).map_err(|e| {
                    SddError::new("E_STATE_CORRUPTED", &format!("验证报告解析失败：{e}"))
                })
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
        warnings.push(json!({
            "code": "W_NO_GIT_SCOPE_CHECK",
            "message": "当前目录不是 git 仓库，未执行 git 事实校验（文件范围、依赖增量、工作区指纹）",
        }));
    }

    // 1. 变更文件扫描（git 可用时）
    let mut changed_files: Vec<String> = Vec::new();
    if is_git {
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
            category: None,
            start_line: None,
            end_line: None,
            existing_code: None,
            suggestion_code: None,
            origin: None,
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
        let plan = crate::state::runtime_store::read_change_field(cwd, &change_id, "plan")?
            .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 plan"))?;
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
    let (audit_max_files, audit_max_bytes) =
        audit_limits(&crate::state::runtime_store::read_config(cwd)?);
    let mut scanned_count = 0usize;
    let mut scanned_bytes = 0usize;
    let mut scan_limited = false;
    for file in &changed_files {
        if scanned_count >= audit_max_files || scanned_bytes >= audit_max_bytes {
            scan_limited = true;
            break;
        }
        let path = GitInspector::resolve_repo_path(&business_cwd, file)?;
        let remaining = audit_max_bytes.saturating_sub(scanned_bytes);
        match read_text_with_limit(&path, remaining) {
            Ok(Some(content)) => {
                scanned_count += 1;
                scanned_bytes = scanned_bytes.saturating_add(content.len());
                if let Err(error) = validate_no_secrets([(file.as_str(), content.as_str())]) {
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
            Ok(None) => {
                scan_limited = true;
                break;
            }
            Err(error) => {
                // 二进制/不可读文件不静默跳过：记录诊断警告
                warnings.push(json!({
                    "code": "W_FILE_UNREADABLE",
                    "message": format!("变更文件 {file} 无法按文本读取（可能是二进制文件）：{error}"),
                    "details": json!({ "file": file }),
                }));
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
        let ocr_config = OcrConfig::from_config(&crate::state::runtime_store::read_config(cwd)?)?;
        match ocr_config.mode {
            OcrMode::Off => {
                set_ocr_status(&mut report, json!({ "status": "off" }));
            }
            OcrMode::Auto | OcrMode::Required => {
                let timeout = ocr_timeout(args);
                let changed_set = changed_files.iter().cloned().collect::<BTreeSet<_>>();
                match executor.execute(
                    std::path::Path::new(&business_cwd),
                    &ocr_config.command,
                    timeout,
                ) {
                    Ok(OcrExecution::NotFound) if ocr_config.mode == OcrMode::Auto => {
                        warnings.push(json!({
                            "code": "W_OCR_NOT_FOUND",
                            "message": "未找到 OCR 命令，已返回确定性审查结果",
                        }));
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
                        match validate_output(
                            output,
                            std::path::Path::new(&business_cwd),
                            &changed_set,
                        ) {
                            Ok(output) => {
                                let metadata = ocr_metadata(&output, changed_files.len());
                                for comment in &output.comments {
                                    issues.push(merge_ocr_comment(comment));
                                }
                                set_ocr_status(&mut report, metadata);
                            }
                            Err(error) => {
                                set_ocr_status(&mut report, json!({ "status": "invalid-output" }));
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

    let report_value = persist_review_report(cwd, &change_id, &change_dir, &report)?;

    let report_text = serde_json::to_string_pretty(&report_value)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("格式化报告失败：{e}")))?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:review-report"),
        "report",
        &format!("runtime://changes/{change_id}/reports/review"),
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
        warnings: if warnings.is_empty() {
            None
        } else {
            Some(warnings)
        },
        action_required: None,
        error: None,
    })
}

/// 以字节上限读取 UTF-8 文本；超限时不返回内容，避免大文件进入内存。
fn read_text_with_limit(path: &Path, maximum: usize) -> Result<Option<String>, std::io::Error> {
    if fs::metadata(path)?.len() > maximum as u64 {
        return Ok(None);
    }
    let mut content = String::new();
    let mut reader = fs::File::open(path)?.take((maximum as u64).saturating_add(1));
    let bytes = reader.read_to_string(&mut content)?;
    Ok((bytes <= maximum).then_some(content))
}

fn ocr_timeout(args: Option<&serde_json::Value>) -> std::time::Duration {
    args.and_then(|value| value.get("timeout"))
        .and_then(serde_json::Value::as_f64)
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .and_then(|seconds| std::time::Duration::try_from_secs_f64(seconds).ok())
        .unwrap_or_else(|| std::time::Duration::from_secs(120))
}

/// 审计扫描限额：config 的 audit.maxSizeMb（默认 5MB）与 audit.maxFiles（默认 200）。
/// 缺省或非法值回退默认，避免把扫描上限压到 0。
fn audit_limits(config: &serde_json::Value) -> (usize, usize) {
    let max_files = config
        .pointer("/audit/maxFiles")
        .and_then(|value| value.as_u64())
        .filter(|count| *count > 0)
        .unwrap_or(200) as usize;
    let max_mb = config
        .pointer("/audit/maxSizeMb")
        .and_then(|value| value.as_u64())
        .filter(|mb| *mb > 0)
        .unwrap_or(5);
    (max_files, (max_mb as usize) * 1024 * 1024)
}

fn set_ocr_status(report: &mut Report, status: serde_json::Value) {
    let minimality = report.minimality.get_or_insert_with(|| json!({}));
    if !minimality.is_object() {
        *minimality = json!({});
    }
    minimality["ocr"] = status;
}

fn ocr_metadata(output: &OcrOutput, fallback_files: usize) -> serde_json::Value {
    json!({
        "status": "completed",
        "sessionId": output.session_id,
        "filesReviewed": output.files_reviewed.unwrap_or(fallback_files as u32),
        "commentCount": output.comments.len(),
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

fn persist_review_report(
    cwd: &str,
    change_id: &str,
    change_dir: &std::path::Path,
    report: &Report,
) -> Result<serde_json::Value, SddError> {
    let report_value = serde_json::to_value(report)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化报告失败：{e}")))?;
    validate_json("report", &report_value)?;
    fs::write(
        change_dir.join("review-report.md"),
        render_report_markdown(report),
    )
    .map_err(|e| {
        SddError::new(
            "E_STATE_CORRUPTED",
            &format!("写入 review-report.md 失败：{e}"),
        )
    })?;
    let mut reports = crate::state::runtime_store::read_change_field(cwd, change_id, "reports")?
        .unwrap_or_else(|| json!({}));
    reports["review"] = report_value.clone();
    crate::state::runtime_store::write_change_field(cwd, change_id, "reports", reports)?;
    Ok(report_value)
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
    use super::{
        audit_limits, cargo_dependency_names, planned_dependency_additions, read_text_with_limit,
    };

    #[test]
    fn audit_limits_default_to_5mb_and_200_files() {
        let (files, bytes) = audit_limits(&serde_json::json!({}));
        assert_eq!(files, 200);
        assert_eq!(bytes, 5 * 1024 * 1024);
    }

    #[test]
    fn audit_limits_read_config_and_ignore_invalid() {
        let config = serde_json::json!({
            "audit": { "maxSizeMb": 2, "maxFiles": 50 }
        });
        let (files, bytes) = audit_limits(&config);
        assert_eq!(files, 50);
        assert_eq!(bytes, 2 * 1024 * 1024);
        // 非法值（0/负数）回退默认
        let (files, _) = audit_limits(&serde_json::json!({ "audit": { "maxFiles": 0 } }));
        assert_eq!(files, 200);
    }

    #[test]
    fn bounded_text_reader_skips_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.txt");
        std::fs::write(&path, "0123456789").unwrap();

        assert_eq!(read_text_with_limit(&path, 9).unwrap(), None);
        assert_eq!(
            read_text_with_limit(&path, 10).unwrap().as_deref(),
            Some("0123456789")
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
