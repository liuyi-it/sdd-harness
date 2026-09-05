//! verify 命令：合并验证与审查，并在失败时派发一轮受控修复。

use std::collections::{BTreeSet, HashSet};
use std::fs;

use serde_json::{json, Value};

use crate::commands::plan::plan_tasks;
use crate::contracts::{AgentActionRequired, CliWarning, CommandResult, VerificationCommand};
use crate::error::SddError;
use crate::git::inspector::RepoEntryContent;
use crate::git::GitInspector;
use crate::quality::report::{render_report_markdown, Issue, Report};
use crate::quality::traceability::{coverage_gaps, extract_spec_ids};
use crate::security::secrets_scanner::validate_no_secrets;
use crate::security::task_scope::validate_file_change;
use crate::state::artifact_store::ArtifactRecord;
use crate::state::file_lock::lock_initialized_sdd;
use crate::state::state_store::{apply_workflow_update, ChangeWorkflow, TASK_STATUS_DONE};

const MAX_RESULT_JSON_BYTES: usize = 4 * 1024 * 1024;

pub fn run_verify(cwd: &str, args: Option<&Value>) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "changeId", "continue", "resultJson"])?;
    let timeout_ms = super::timeout_ms(args)?;
    let _guard = lock_initialized_sdd(cwd, timeout_ms)?;
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let change_id = super::resolve_change_id(&runtime, args)?;
    let workflow = super::workflow(&runtime, &change_id)?;
    super::ensure_phase(workflow, "verify", &change_id)?;
    let continue_fix = super::bool_arg(args, "continue")?.unwrap_or(false);
    let result_json = super::string_arg(args, "resultJson")?;

    if continue_fix {
        if result_json.is_some() || workflow.phase != "QUALITY_BLOCKED" {
            return Err(SddError::new(
                "E_INVALID_PHASE_COMMAND",
                "--continue 仅用于用户明确授权 QUALITY_BLOCKED 的下一轮修复",
            ));
        }
        return start_fix(cwd, &runtime, &change_id, true);
    }
    if let Some(raw) = result_json {
        if workflow.phase != "QUALITY_WAITING_FIX" {
            return Err(SddError::new(
                "E_INVALID_PHASE_COMMAND",
                "当前没有等待提交的质量修复结果",
            ));
        }
        return complete_fix(cwd, &runtime, &change_id, raw);
    }
    if workflow.phase == "QUALITY_WAITING_FIX" {
        return fix_action(cwd, &runtime, &change_id);
    }

    let (report, warnings) = assess(cwd, &runtime, &change_id)?;
    record_report(cwd, &change_id, &report)?;
    if report.passed {
        set_ready(cwd, &change_id)?;
        return ready_result(change_id, report, warnings);
    }
    if workflow.quality_fix_rounds == 0 {
        return start_fix(cwd, &runtime, &change_id, false);
    }
    set_blocked(cwd, &change_id, &report.summary)?;
    blocked_result(change_id, report, warnings)
}

fn complete_fix(
    cwd: &str,
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
    raw: &str,
) -> Result<CommandResult, SddError> {
    if raw.len() > MAX_RESULT_JSON_BYTES {
        return Err(SddError::new(
            "E_QUALITY_FAILED",
            &format!("resultJson 超过 {MAX_RESULT_JSON_BYTES} 字节上限"),
        ));
    }
    let value: Value = serde_json::from_str(raw).map_err(|error| {
        SddError::new(
            "E_QUALITY_FAILED",
            &format!("修复结果不是合法 JSON：{error}"),
        )
    })?;
    crate::schema::validate_json("fix-result", &value)
        .map_err(|error| SddError::new("E_QUALITY_FAILED", &error.message))?;
    let workflow = super::workflow(runtime, change_id)?;
    let pending = workflow
        .pending_agent_action
        .as_ref()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "QUALITY_WAITING_FIX 缺少待处理修复"))?;
    let expected_fix_id = pending
        .get("fixId")
        .and_then(Value::as_str)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "待处理修复缺少 fixId"))?;
    if value.get("fixId").and_then(Value::as_str) != Some(expected_fix_id) {
        return Err(SddError::new("E_QUALITY_FAILED", "修复结果的 fixId 不匹配"));
    }
    let allowed = allowed_files(runtime, change_id)?;
    let files = string_array(&value, "filesChanged")?;
    validate_file_change(&files, &allowed, &[], &[])?;
    validate_fix_verification(runtime, change_id, &value)?;
    if value.get("status").and_then(Value::as_str) != Some("completed") {
        set_blocked(cwd, change_id, "Agent 未能完成质量修复")?;
        let (report, warnings) = assess(cwd, runtime, change_id)?;
        record_report(cwd, change_id, &report)?;
        return blocked_result(change_id.to_string(), report, warnings);
    }

    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        let workflow = super::workflow_mut(document, change_id)?;
        apply_workflow_update(workflow, |workflow| {
            workflow.pending_agent_action = None;
            workflow.phase = "BUILD_READY".to_string();
            workflow.in_progress_phase = Some("QUALITY".to_string());
            workflow.last_command = Some("sdd verify".to_string());
        })
    })?;
    let current = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let (report, warnings) = assess(cwd, &current, change_id)?;
    record_report(cwd, change_id, &report)?;
    if report.passed {
        set_ready(cwd, change_id)?;
        ready_result(change_id.to_string(), report, warnings)
    } else {
        set_blocked(cwd, change_id, &report.summary)?;
        blocked_result(change_id.to_string(), report, warnings)
    }
}

fn start_fix(
    cwd: &str,
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
    user_authorized: bool,
) -> Result<CommandResult, SddError> {
    let workflow = super::workflow(runtime, change_id)?;
    let next_round = workflow
        .quality_fix_rounds
        .checked_add(1)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "质量修复轮次溢出"))?;
    let fix_id = format!("FIX-{next_round:03}");
    let pending = json!({
        "type": "AGENT_FIX_EXECUTION",
        "fixId": fix_id,
        "since": crate::state::state_store::now_iso(),
        "userAuthorized": user_authorized,
    });
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        let workflow = super::workflow_mut(document, change_id)?;
        apply_workflow_update(workflow, |workflow| {
            workflow.phase = "QUALITY_WAITING_FIX".to_string();
            workflow.in_progress_phase = Some("QUALITY_FIX".to_string());
            workflow.pending_agent_action = Some(pending.clone());
            workflow.quality_fix_rounds = next_round;
            workflow.suggested_command = Some(format!(
                "sdd verify --change {change_id} --result-json '<JSON>'"
            ));
            workflow.last_command = Some("sdd verify".to_string());
        })
    })?;
    let current = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    fix_action(cwd, &current, change_id)
}

fn fix_action(
    cwd: &str,
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
) -> Result<CommandResult, SddError> {
    let workflow = super::workflow(runtime, change_id)?;
    let fix_id = workflow
        .pending_agent_action
        .as_ref()
        .and_then(|value| value.get("fixId"))
        .and_then(Value::as_str)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "待处理修复缺少 fixId"))?;
    let report = runtime
        .changes
        .get(change_id)
        .and_then(|change| change.get("reports"))
        .and_then(|reports| reports.get("quality"))
        .cloned()
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "缺少质量报告"))?;
    let verification = verification_commands(runtime, change_id)?;
    let schema: Value = serde_json::from_str(crate::schema::schema_source("fix-result")?)
        .expect("内嵌 fix-result schema 必须合法");
    let docs = read_documents(cwd, change_id)?;
    Ok(CommandResult {
        ok: true,
        state: "QUALITY_WAITING_FIX".to_string(),
        exit_code: 0,
        change_id: Some(change_id.to_string()),
        next: Some(format!(
            "sdd verify --change {change_id} --result-json '<JSON>'"
        )),
        data: Some(json!({ "report": report })),
        rendered: None,
        warnings: None,
        action_required: Some(AgentActionRequired::AgentFixExecution {
            fix_id: fix_id.to_string(),
            change_id: change_id.to_string(),
            context_pack: format!(
                "# 质量修复\n\n## 质量报告\n\n{}\n\n## 已批准文档\n\n{docs}\n\n只修复报告中的阻断问题，不扩大需求范围；完成后执行全部 verification 并回传 inline JSON。",
                serde_json::to_string_pretty(&report).expect("质量报告必须可序列化")
            ),
            allowed_files: allowed_files(runtime, change_id)?,
            verification,
            result_schema: schema,
            result_transport: "inline-json".to_string(),
        }),
        error: None,
    })
}

fn assess(
    cwd: &str,
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
) -> Result<(Report, Vec<CliWarning>), SddError> {
    crate::state::artifact_store::verify_artifacts_in(
        cwd,
        runtime,
        [
            format!("{change_id}:spec"),
            format!("{change_id}:plan"),
            format!("{change_id}:plan-md"),
            format!("{change_id}:tasks-md"),
        ],
    )?;
    let workflow = super::workflow(runtime, change_id)?;
    let change = runtime
        .changes
        .get(change_id)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime 缺少 change"))?;
    let model = crate::engines::spec::model_from_record(
        change
            .get("spec")
            .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime 缺少 spec"))?,
    )?;
    let tasks = plan_tasks(
        change
            .get("plan")
            .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime 缺少 plan"))?,
    )?;
    crate::commands::build::validate_runtime_task_state(workflow, &tasks)?;
    let done_ids = workflow
        .tasks
        .iter()
        .filter(|(_, status)| status.as_str() == TASK_STATUS_DONE)
        .map(|(task_id, _)| task_id.clone())
        .collect::<HashSet<_>>();
    let (requirement_ids, scenario_ids) = extract_spec_ids(&model);
    let mut issues = coverage_gaps(&requirement_ids, &scenario_ids, &tasks, &done_ids)
        .into_iter()
        .map(|message| issue("E_VERIFY_REQUIRED", "high", message, None))
        .collect::<Vec<_>>();
    let results = runtime
        .runs
        .get(&workflow.run_id)
        .and_then(|run| run.get("tasks"))
        .and_then(Value::as_object)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "run.tasks 必须是对象"))?;
    for task in &tasks {
        let valid = results
            .get(&task.id)
            .and_then(|result| crate::protocol::validate_task_result(result).ok())
            .is_some_and(|parsed| {
                parsed.status == "completed"
                    && crate::commands::build::validate_task_evidence(task, &parsed).is_ok()
            });
        if !valid {
            issues.push(issue(
                "E_TDD_EVIDENCE_REQUIRED",
                "high",
                format!("任务 {} 缺少有效完成证据", task.id),
                Some(format!(
                    "runtime://runs/{}/tasks/{}",
                    workflow.run_id, task.id
                )),
            ));
        }
    }

    let business_cwd = business_root(cwd, workflow);
    let mut warnings = Vec::new();
    let changed_files = if GitInspector::is_git_repo(&business_cwd)? {
        let workspace = workflow
            .workspace
            .as_ref()
            .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "Git 工作流缺少 workspace 基线"))?;
        GitInspector::changes_since(
            &business_cwd,
            &workspace.baseline_changed_files,
            &workspace.baseline_file_hashes,
        )?
    } else {
        warnings.push(CliWarning::new(
            "W_NO_GIT_SCOPE_CHECK",
            "当前目录不是 Git 仓库，无法核验实际变更范围",
        ));
        Vec::new()
    };
    // 禁止范围只约束各自任务的增量，不能跨任务合并；任务完成时已按其基线校验。
    // 全局门禁仅确认每个实际变更至少被某个任务允许，避免互斥任务范围相互误杀。
    if let Err(error) = validate_file_change(
        &changed_files,
        &allowed_files(runtime, change_id)?,
        &[],
        &[],
    ) {
        issues.push(issue(&error.code, "critical", error.message, None));
    }
    scan_changed_files(runtime, &business_cwd, &changed_files, &mut issues)?;
    validate_dependencies(
        runtime,
        change_id,
        &business_cwd,
        &changed_files,
        &mut issues,
    )?;

    let blocking = issues
        .iter()
        .filter(|item| matches!(item.severity.as_str(), "critical" | "high"))
        .count();
    let mut report = Report::new("quality", change_id.to_string());
    report.passed = blocking == 0;
    report.summary = if report.passed {
        format!(
            "质量门禁通过：{} 个需求、{} 个场景、{} 个任务",
            requirement_ids.len(),
            scenario_ids.len(),
            tasks.len()
        )
    } else {
        format!("发现 {blocking} 个阻断问题")
    };
    report.issues = issues;
    report.minimality = Some(json!({
        "changedFiles": changed_files,
        "gitFingerprint": if GitInspector::is_git_repo(&business_cwd)? {
            Some(GitInspector::workspace_fingerprint(&business_cwd)?)
        } else {
            None
        },
    }));
    let value = serde_json::to_value(&report)
        .map_err(|error| SddError::new("E_STATE_CORRUPTED", &error.to_string()))?;
    crate::schema::validate_json("report", &value)?;
    Ok((report, warnings))
}

fn scan_changed_files(
    runtime: &crate::state::RuntimeDocument,
    business_cwd: &str,
    changed_files: &[String],
    issues: &mut Vec<Issue>,
) -> Result<(), SddError> {
    let (max_files, max_bytes) = audit_limits(&runtime.config)?;
    let mut scanned_bytes = 0usize;
    for (index, file) in changed_files.iter().enumerate() {
        if index >= max_files || scanned_bytes >= max_bytes {
            issues.push(issue(
                "E_AUDIT_SCAN_INCOMPLETE",
                "critical",
                "变更文件超过审计扫描上限".to_string(),
                None,
            ));
            break;
        }
        match GitInspector::read_entry_with_limit(business_cwd, file, max_bytes - scanned_bytes)? {
            RepoEntryContent::Content(bytes) => {
                scanned_bytes += bytes.len();
                let content = String::from_utf8_lossy(&bytes);
                if let Err(error) = validate_no_secrets([(file.as_str(), content.as_ref())]) {
                    issues.push(issue(
                        &error.code,
                        "critical",
                        error.message,
                        Some(file.clone()),
                    ));
                }
            }
            RepoEntryContent::Missing => {}
            RepoEntryContent::TooLarge => issues.push(issue(
                "E_AUDIT_SCAN_INCOMPLETE",
                "critical",
                format!("文件 {file} 超过审计扫描上限"),
                Some(file.clone()),
            )),
        }
    }
    Ok(())
}

fn validate_dependencies(
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
    business_cwd: &str,
    changed_files: &[String],
    issues: &mut Vec<Issue>,
) -> Result<(), SddError> {
    if !changed_files.iter().any(|path| path == "Cargo.toml") {
        return Ok(());
    }
    let workflow = super::workflow(runtime, change_id)?;
    let baseline = workflow
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.baseline_cargo_manifest.as_deref())
        .unwrap_or("");
    let current = fs::read_to_string(GitInspector::resolve_repo_path(business_cwd, "Cargo.toml")?)
        .unwrap_or_default();
    let added = cargo_dependency_names(&current)
        .difference(&cargo_dependency_names(baseline))
        .cloned()
        .collect::<BTreeSet<_>>();
    let declared = runtime
        .changes
        .get(change_id)
        .and_then(|change| change.get("plan"))
        .and_then(|plan| plan.get("dependencies"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("action").and_then(Value::as_str) == Some("ADD"))
        .filter_map(|item| item.get("name").and_then(Value::as_str))
        .map(String::from)
        .collect::<BTreeSet<_>>();
    let unplanned = added.difference(&declared).cloned().collect::<Vec<_>>();
    if !unplanned.is_empty() {
        issues.push(issue(
            "E_UNPLANNED_DEPENDENCY",
            "high",
            format!("新增依赖未在计划中声明：{}", unplanned.join("、")),
            Some("Cargo.toml".to_string()),
        ));
    }
    Ok(())
}

fn record_report(cwd: &str, change_id: &str, report: &Report) -> Result<(), SddError> {
    let change_dir = crate::state::paths::change_dir(cwd, change_id, false)?;
    crate::safe_fs::atomic_write(
        &change_dir.join("quality-report.md"),
        render_report_markdown(report).as_bytes(),
        "quality-report.md",
    )?;
    let value = serde_json::to_value(report)
        .map_err(|error| SddError::new("E_STATE_CORRUPTED", &error.to_string()))?;
    let artifact_key = format!("{change_id}:quality-report");
    let content_path = format!("runtime://changes/{change_id}/reports/quality");
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        super::reports_mut(super::change_mut(document, change_id)?)?
            .insert("quality".to_string(), value.clone());
        crate::state::artifact_store::record_artifacts_in(
            cwd,
            document,
            vec![ArtifactRecord {
                key: &artifact_key,
                artifact_type: "report",
                content_path: &content_path,
                inputs: json!({ "kind": "quality" }),
            }],
        )
    })?;
    Ok(())
}

fn set_ready(cwd: &str, change_id: &str) -> Result<(), SddError> {
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        let workflow = super::workflow_mut(document, change_id)?;
        apply_workflow_update(workflow, |workflow| {
            workflow.phase = "QUALITY_READY".to_string();
            workflow.in_progress_phase = None;
            workflow.pending_agent_action = None;
            workflow.suggested_command = Some(format!("sdd archive --change {change_id}"));
            workflow.last_command = Some("sdd verify".to_string());
            workflow.clear_failure();
        })
    })?;
    Ok(())
}

fn set_blocked(cwd: &str, change_id: &str, reason: &str) -> Result<(), SddError> {
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        let workflow = super::workflow_mut(document, change_id)?;
        apply_workflow_update(workflow, |workflow| {
            workflow.phase = "QUALITY_BLOCKED".to_string();
            workflow.in_progress_phase = None;
            workflow.pending_agent_action = None;
            workflow.record_failure("sdd verify", reason);
            workflow.suggested_command =
                Some(format!("sdd verify --change {change_id} --continue"));
            workflow.last_command = Some("sdd verify".to_string());
        })
    })?;
    Ok(())
}

fn ready_result(
    change_id: String,
    report: Report,
    warnings: Vec<CliWarning>,
) -> Result<CommandResult, SddError> {
    Ok(CommandResult {
        ok: true,
        state: "QUALITY_READY".to_string(),
        exit_code: 0,
        change_id: Some(change_id.clone()),
        next: Some(format!("sdd archive --change {change_id}")),
        data: Some(json!({ "report": report })),
        rendered: None,
        warnings: (!warnings.is_empty()).then_some(warnings),
        action_required: None,
        error: None,
    })
}

fn blocked_result(
    change_id: String,
    report: Report,
    warnings: Vec<CliWarning>,
) -> Result<CommandResult, SddError> {
    Ok(CommandResult {
        ok: false,
        state: "QUALITY_BLOCKED".to_string(),
        exit_code: crate::contracts::error_exit_codes("E_QUALITY_FAILED"),
        change_id: Some(change_id.clone()),
        next: Some(format!("sdd verify --change {change_id} --continue")),
        data: Some(json!({ "report": report })),
        rendered: None,
        warnings: (!warnings.is_empty()).then_some(warnings),
        action_required: None,
        error: None,
    })
}

fn issue(code: &str, severity: &str, message: String, file: Option<String>) -> Issue {
    Issue {
        code: code.to_string(),
        severity: severity.to_string(),
        message,
        file,
        category: None,
        start_line: None,
        end_line: None,
        existing_code: None,
        suggestion_code: None,
        origin: None,
    }
}

fn allowed_files(
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
) -> Result<Vec<String>, SddError> {
    Ok(read_tasks(runtime, change_id)?
        .into_iter()
        .flat_map(|task| task.allowed_files)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn verification_commands(
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
) -> Result<Vec<VerificationCommand>, SddError> {
    let mut commands = BTreeSet::new();
    for task in read_tasks(runtime, change_id)? {
        for item in task.verification {
            commands.insert((item.command, item.args));
        }
    }
    Ok(commands
        .into_iter()
        .map(|(command, args)| VerificationCommand { command, args })
        .collect())
}

fn validate_fix_verification(
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
    value: &Value,
) -> Result<(), SddError> {
    let expected = verification_commands(runtime, change_id)?
        .into_iter()
        .map(|item| (item.command, item.args))
        .collect::<BTreeSet<_>>();
    let submitted = value
        .get("verification")
        .and_then(Value::as_array)
        .ok_or_else(|| SddError::new("E_QUALITY_FAILED", "verification 必须是数组"))?;
    let mut actual = BTreeSet::new();
    for item in submitted {
        let command = item
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| SddError::new("E_QUALITY_FAILED", "verification 缺少 command"))?;
        let args = string_array(item, "args")?;
        if item.get("passed").and_then(Value::as_bool) != Some(true) {
            return Err(SddError::new(
                "E_QUALITY_FAILED",
                "质量修复后的验证必须全部通过",
            ));
        }
        actual.insert((command.to_string(), args));
    }
    if actual != expected {
        return Err(SddError::new(
            "E_QUALITY_FAILED",
            "质量修复必须执行全部计划验证命令",
        ));
    }
    Ok(())
}

fn string_array(value: &Value, field: &str) -> Result<Vec<String>, SddError> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| SddError::new("E_QUALITY_FAILED", &format!("{field} 必须是数组")))?
        .iter()
        .map(|item| {
            item.as_str().map(String::from).ok_or_else(|| {
                SddError::new("E_QUALITY_FAILED", &format!("{field} 必须是字符串数组"))
            })
        })
        .collect()
}

fn read_tasks(
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
) -> Result<Vec<crate::engines::tdd::TaskDefinition>, SddError> {
    plan_tasks(
        runtime
            .changes
            .get(change_id)
            .and_then(|change| change.get("plan"))
            .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime 缺少 plan"))?,
    )
}

fn business_root(cwd: &str, workflow: &ChangeWorkflow) -> String {
    workflow
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.worktree_path.clone())
        .unwrap_or_else(|| cwd.to_string())
}

fn read_documents(cwd: &str, change_id: &str) -> Result<String, SddError> {
    let dir = crate::state::paths::change_dir(cwd, change_id, false)?;
    ["spec.md", "plan.md", "tasks.md", "quality-report.md"]
        .iter()
        .map(|name| {
            fs::read_to_string(dir.join(name))
                .map(|content| format!("## {name}\n\n{content}"))
                .map_err(|error| SddError::new("E_MISSING_ARTIFACT", &error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|items| items.join("\n\n"))
}

fn audit_limits(config: &Value) -> Result<(usize, usize), SddError> {
    let max_files = config
        .pointer("/audit/maxFiles")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "audit.maxFiles 必须是正整数"))?;
    let max_bytes = config
        .pointer("/audit/maxSizeMb")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .and_then(|value| usize::try_from(value).ok())
        .and_then(|value| value.checked_mul(1024 * 1024))
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "audit.maxSizeMb 非法"))?;
    Ok((max_files, max_bytes))
}

fn cargo_dependency_names(content: &str) -> BTreeSet<String> {
    let mut section = "";
    let mut dependencies = BTreeSet::new();
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            section = line.trim_matches(['[', ']']).trim();
            continue;
        }
        if matches!(
            section,
            "dependencies" | "dev-dependencies" | "build-dependencies"
        ) || section.ends_with(".dependencies")
            || section.ends_with(".dev-dependencies")
            || section.ends_with(".build-dependencies")
        {
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
