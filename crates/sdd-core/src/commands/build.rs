//! build 命令：获取下一个任务（build next）或提交 Agent 结果（build complete）。
//!
//! 核心裁决语义：
//! - next：找下一个可执行任务，写 pendingAgentTask，返回 actionRequired
//! - complete：校验结果结构/任务身份/TDD evidence，写运行级结果并推进任务状态
//!
//! 契约变更点：actionRequired.codebase.provider 为
//! codegraph | fallback-file-scan。

use serde_json::json;
use std::fs;

use crate::commands::new::current_change_id;
use crate::commands::plan::plan_tasks;
use crate::contracts::{
    AgentActionRequired, CliWarning, CodebaseProviderInfo, CommandResult, VerificationCommand,
};
use crate::engines::tdd::TaskDefinition;
use crate::error::SddError;
use crate::git::GitInspector;
use crate::protocol::{validate_task_result, TaskExecutionResult};
use crate::security::task_scope::validate_file_change;
use crate::state::artifact_store::ArtifactRecord;
use crate::state::file_lock::lock_initialized_sdd;
use crate::state::state_store::{
    TASK_STATUS_BUILDING, TASK_STATUS_DONE, TASK_STATUS_FAILED, TASK_STATUS_PENDING,
};

const MAX_RESULT_JSON_BYTES: usize = 4 * 1024 * 1024;

pub fn run_build(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "changeId", "sub", "task", "resultJson"])?;
    let empty = serde_json::Value::Null;
    let args = args.unwrap_or(&empty);
    let timeout_ms = super::timeout_ms(Some(args))?;
    let sub = super::string_arg(Some(args), "sub")?.unwrap_or("next");
    match sub {
        "next" => {
            if args.get("task").is_some() || args.get("resultJson").is_some() {
                return Err(SddError::new(
                    "E_INVALID_PHASE_COMMAND",
                    "build next 不接受 task 或 resultJson",
                ));
            }
            run_build_next(cwd, args, timeout_ms)
        }
        "complete" => {
            let task_id = super::string_arg(Some(args), "task")?.ok_or_else(|| {
                SddError::new("E_INVALID_PHASE_COMMAND", "build complete 需要 --task <id>")
            })?;
            let result_json = super::string_arg(Some(args), "resultJson")?
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    SddError::new(
                        "E_INVALID_PHASE_COMMAND",
                        "build complete 需要 --result-json '<TaskExecutionResult JSON>'",
                    )
                })?;
            if result_json.len() > MAX_RESULT_JSON_BYTES {
                return Err(SddError::new(
                    "E_TDD_EVIDENCE_REQUIRED",
                    &format!("resultJson 超过 {MAX_RESULT_JSON_BYTES} 字节上限"),
                ));
            }
            run_build_complete(cwd, args, task_id, result_json, timeout_ms)
        }
        _ => Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("未知 build 子命令：{sub}"),
        )),
    }
}

/// build next：返回下一个任务的 actionRequired
fn run_build_next(
    cwd: &str,
    args: &serde_json::Value,
    timeout_ms: Option<u64>,
) -> Result<CommandResult, SddError> {
    let _guard = lock_initialized_sdd(cwd, "sdd build next", None, timeout_ms)?;
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let state = runtime.state.clone();
    super::ensure_phase(cwd, &state, "build", Some(args))?;
    let business_cwd = business_root(cwd, &state);
    let change_id = current_change_id(&state)?;
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
    let plan = runtime
        .changes
        .get(&change_id)
        .and_then(|change| change.get("plan"))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 plan"))?;
    let tasks = plan_tasks(plan)?;
    validate_runtime_task_state(&state, &tasks)?;

    // 续跑：已有 pendingAgentTask 时直接返回
    if state.current_phase == "BUILD_WAITING_AGENT" && state.pending_agent_task.is_some() {
        if let Some(pending) = &state.pending_agent_task {
            let task_id = pending
                .get("taskId")
                .and_then(|value| value.as_str())
                .ok_or_else(|| {
                    SddError::new("E_STATE_CORRUPTED", "pendingAgentTask 缺少 taskId")
                })?;
            let task = tasks
                .iter()
                .find(|task| task.id == task_id)
                .cloned()
                .ok_or_else(|| {
                    SddError::new("E_MISSING_ARTIFACT", &format!("计划中不存在任务 {task_id}"))
                })?;
            validate_task_verification(&task)?;
            let context_pack = render_context_pack(cwd, &change_id, &task, &runtime)?;
            return Ok(action_required_result(
                task,
                change_id,
                context_pack,
                state.codebase_provider.clone(),
                state.degraded,
            ));
        }
    }
    if state.current_phase != "PLAN_READY" && state.current_phase != "BUILD_WAITING_AGENT" {
        // 错误建议改用阶段表：全部任务完成后（BUILD_READY）应提示 sdd verify
        let next = crate::commands::status::next_command(&state.current_phase)
            .unwrap_or_else(|| "sdd plan".to_string());
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("无法在 {} 状态下获取任务", state.current_phase),
        )
        .with_next(&next));
    }

    // 找下一个可执行任务：以运行时状态（state.tasks）为准，
    // 所有依赖 DONE、自身 PENDING
    let next_task = tasks
        .iter()
        .find(|task| {
            let runtime_status = state
                .tasks
                .get(&task.id)
                .map(|s| s.as_str())
                .expect("validate_runtime_task_state 已确认任务状态存在");
            // PENDING 或 FAILED（失败后允许重新派发）且依赖已完成
            (runtime_status == TASK_STATUS_PENDING || runtime_status == TASK_STATUS_FAILED)
                && task.depends_on.iter().all(|dep| {
                    state
                        .tasks
                        .get(dep)
                        .map(|s| s == TASK_STATUS_DONE)
                        .unwrap_or(false)
                })
        })
        .ok_or_else(|| {
            SddError::new(
                "E_INVALID_PHASE_COMMAND",
                "没有可执行的 PENDING 任务（任务可能全部完成或存在循环依赖）",
            )
        })?;
    // 派发前校验每条验证命令（防计划被篡改为任意命令）
    validate_task_verification(next_task)?;
    // 写 pendingAgentTask + 状态推进
    let git_baseline = if GitInspector::is_git_repo(&business_cwd)? {
        let head = GitInspector::head(&business_cwd)?;
        let changed_files = business_changes(&business_cwd)?;
        json!({
            "available": true,
            "head": head,
            "changedFiles": changed_files,
            "changedFileHashes": GitInspector::file_hashes(&business_cwd, &changed_files)?,
        })
    } else {
        json!({ "available": false })
    };
    let context_pack = render_context_pack(cwd, &change_id, next_task, &runtime)?;
    let task_id = next_task.id.clone();
    let pending_task = json!({
        "taskId": task_id,
        "since": crate::state::state_store::now_iso(),
        "gitBaseline": git_baseline,
    });
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(move |document| {
        crate::state::state_store::apply_state_update(&mut document.state, |state| {
            state.current_phase = "BUILD_WAITING_AGENT".to_string();
            state.in_progress_phase = Some("BUILDING".to_string());
            state.clear_failure();
            state.suggested_command = Some("sdd build complete".to_string());
            state.last_command = Some("sdd build next".to_string());
            state.pending_agent_task = Some(pending_task);
            state
                .tasks
                .insert(task_id, TASK_STATUS_BUILDING.to_string());
        })?;
        Ok(())
    })?;
    Ok(action_required_result(
        next_task.clone(),
        change_id,
        context_pack,
        state.codebase_provider,
        state.degraded,
    ))
}

/// 构造 actionRequired 结果（结果通过 inline JSON 提交）。
fn action_required_result(
    task: TaskDefinition,
    change_id: String,
    context_pack: String,
    provider: String,
    degraded: bool,
) -> CommandResult {
    let verification: Vec<VerificationCommand> = task
        .verification
        .iter()
        .map(|cmd| {
            let mut parts = cmd.split_whitespace();
            VerificationCommand {
                command: parts
                    .next()
                    .expect("validate_task_verification 已拒绝空命令")
                    .to_string(),
                args: parts.map(String::from).collect(),
            }
        })
        .collect();
    let policies = crate::policies::builtin_build_policies();
    let policy_bundle = json!({
        "policies": policies.iter().map(|policy| json!({
            "name": policy.name,
            "digest": policy.digest,
            "prompt": policy.source,
        })).collect::<Vec<_>>()
    });
    let TaskDefinition {
        id,
        allowed_files,
        expected_new_files,
        forbidden_files,
        ..
    } = task;
    CommandResult {
        ok: true,
        state: "BUILD_WAITING_AGENT".to_string(),
        exit_code: 0,
        change_id: Some(change_id.clone()),
        next: Some("sdd build complete".to_string()),
        data: None,
        rendered: None,
        warnings: None,
        action_required: Some(AgentActionRequired {
            action_type: "AGENT_TASK_EXECUTION".to_string(),
            task_id: id,
            change_id,
            context_pack,
            allowed_files,
            expected_new_files,
            forbidden_files,
            verification,
            result_transport: "inline-json".to_string(),
            codebase: CodebaseProviderInfo { provider, degraded },
            policy_bundle: Some(policy_bundle),
        }),
        error: None,
    }
}

/// build complete：校验并推进任务
fn run_build_complete(
    cwd: &str,
    args: &serde_json::Value,
    task_id: &str,
    result_json: &str,
    timeout_ms: Option<u64>,
) -> Result<CommandResult, SddError> {
    let _guard = lock_initialized_sdd(cwd, "sdd build complete", None, timeout_ms)?;
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let state = runtime.state.clone();
    super::ensure_phase(cwd, &state, "build", Some(args))?;
    let business_cwd = business_root(cwd, &state);
    let change_id = current_change_id(&state)?;
    crate::state::artifact_store::verify_artifacts_in(
        cwd,
        &runtime,
        [format!("{change_id}:plan")],
    )?;
    if state.current_phase != "BUILD_WAITING_AGENT" {
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("无法在 {} 状态下提交任务结果", state.current_phase),
        )
        .with_next("sdd build next"));
    }
    // 任务身份校验：pendingAgentTask 必须匹配
    let pending = state.pending_agent_task.as_ref().ok_or_else(|| {
        SddError::new(
            "E_STATE_CORRUPTED",
            "BUILD_WAITING_AGENT 状态缺少 pendingAgentTask",
        )
    })?;
    let pending_task_id = pending
        .get("taskId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "pendingAgentTask 缺少 taskId"))?;
    if pending_task_id != task_id {
        return Err(SddError::new(
            "E_AGENT_TASK_FAILED",
            &format!("任务 {task_id} 与当前等待的任务 {pending_task_id} 不一致"),
        )
        .with_next("sdd build next"));
    }

    // 读取并校验 inline JSON 结果。
    let result: serde_json::Value = serde_json::from_str(result_json).map_err(|e| {
        SddError::new(
            "E_TDD_EVIDENCE_REQUIRED",
            &format!("任务 {task_id} 的执行结果结构无效：{e}"),
        )
    })?;
    let mut warnings = Vec::new();
    let parsed = validate_task_result(&result)?;
    if parsed.task_id != task_id {
        return Err(SddError::new(
            "E_AGENT_TASK_FAILED",
            &format!(
                "执行结果中的 taskId（{}）与请求的任务（{task_id}）不一致",
                parsed.task_id
            ),
        ));
    }
    let status = parsed.status.as_str();

    let plan = runtime
        .changes
        .get(&change_id)
        .and_then(|change| change.get("plan"))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 plan"))?;
    let tasks = plan_tasks(plan)?;
    validate_runtime_task_state(&state, &tasks)?;
    let task = tasks.iter().find(|t| t.id == task_id).ok_or_else(|| {
        SddError::new("E_MISSING_ARTIFACT", &format!("计划中不存在任务 {task_id}"))
    })?;

    // TDD evidence 校验（RED 需要失败证据、GREEN 需要通过证据）
    validate_task_evidence(task, &parsed)?;

    for path in &parsed.files_changed {
        GitInspector::resolve_repo_path(&business_cwd, path)?;
    }
    let actual_files = if GitInspector::is_git_repo(&business_cwd)? {
        let actual = task_changes(&business_cwd, pending)?;
        let mut declared = parsed.files_changed.clone();
        let mut actual_sorted = actual.clone();
        declared.sort();
        declared.dedup();
        actual_sorted.sort();
        actual_sorted.dedup();
        if declared != actual_sorted {
            return Err(SddError::new(
                "E_UNDECLARED_FILE_CHANGE",
                &format!(
                    "Agent 声明的 filesChanged 与 Git 事实不一致（声明：{}；实际：{}）",
                    declared.join("、"),
                    actual_sorted.join("、")
                ),
            ));
        }
        actual
    } else {
        // 非 git 仓库：沿用声明 filesChanged，但明确提示缺少 Git 事实核对
        if !parsed.files_changed.is_empty() {
            warnings.push(CliWarning::new(
                "W_NO_GIT_FACTS",
                "当前目录不是 git 仓库，无法用 Git 事实核对 filesChanged 声明",
            ));
        }
        parsed.files_changed.clone()
    };
    validate_file_change(
        &actual_files,
        &task.allowed_files,
        &task.expected_new_files,
        &task.forbidden_files,
    )?;

    // 写入 runtime 的 runs.<runId>.tasks.<taskId>。
    let run_id = state.current_run_id.clone().ok_or_else(|| {
        SddError::new(
            "E_STATE_CORRUPTED",
            "BUILD_WAITING_AGENT 状态缺少 currentRunId",
        )
    })?;
    let task_status = if status == "completed" {
        TASK_STATUS_DONE
    } else {
        TASK_STATUS_FAILED
    };
    let all_done = tasks.iter().all(|t| {
        if t.id == task_id {
            status == "completed"
        } else {
            state
                .tasks
                .get(&t.id)
                .map(|s| s == TASK_STATUS_DONE)
                .unwrap_or(false)
        }
    });
    let artifact_key = format!("{run_id}:{task_id}:result");
    let content_path = format!("runtime://runs/{run_id}/tasks/{task_id}");
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(move |document| {
        {
            let tasks = document
                .runs
                .get_mut(&run_id)
                .and_then(serde_json::Value::as_object_mut)
                .and_then(|run| run.get_mut("tasks"))
                .and_then(serde_json::Value::as_object_mut)
                .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "当前 run.tasks 必须是对象"))?;
            tasks.insert(task_id.to_string(), result);
        }
        crate::state::artifact_store::record_artifacts_in(
            cwd,
            document,
            vec![ArtifactRecord {
                key: &artifact_key,
                artifact_type: "report",
                content_path: &content_path,
                inputs: json!({ "taskId": task_id }),
            }],
        )?;
        crate::state::state_store::apply_state_update(&mut document.state, |state| {
            state
                .tasks
                .insert(task_id.to_string(), task_status.to_string());
            state.pending_agent_task = None;
            if status == "failed" {
                // 任务失败：回到 PLAN_READY，FAILED 任务可由 build next 重新派发
                state.current_phase = "PLAN_READY".to_string();
                state.record_failure("sdd build complete", format!("任务 {task_id} 执行失败"));
                state.suggested_command = Some("sdd build next".to_string());
            } else if all_done {
                state.current_phase = "BUILD_READY".to_string();
                state.in_progress_phase = None;
                state.clear_failure();
                state.suggested_command = Some("sdd verify".to_string());
            } else {
                state.current_phase = "PLAN_READY".to_string();
                state.clear_failure();
                state.suggested_command = Some("sdd build next".to_string());
            }
            state.last_command = Some("sdd build complete".to_string());
        })?;
        Ok(())
    })?;

    let next = if status == "failed" {
        "sdd build next"
    } else if all_done {
        "sdd verify"
    } else {
        "sdd build next"
    };
    Ok(CommandResult {
        ok: status == "completed",
        state: if all_done {
            "BUILD_READY".to_string()
        } else {
            "PLAN_READY".to_string()
        },
        exit_code: if status == "failed" { 7 } else { 0 },
        change_id: Some(change_id),
        next: Some(next.to_string()),
        data: Some(json!({ "taskId": task_id, "status": task_status })),
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

/// 校验任务声明中的每条验证命令都在允许范围内（防计划被篡改后派发任意命令）。
fn validate_task_verification(task: &TaskDefinition) -> Result<(), SddError> {
    for verification in &task.verification {
        crate::security::verification_command::validate_verification_command(verification)?;
    }
    Ok(())
}

pub(crate) fn validate_runtime_task_state(
    state: &crate::state::WorkflowState,
    tasks: &[TaskDefinition],
) -> Result<(), SddError> {
    if state.tasks.len() != tasks.len()
        || tasks.iter().any(|task| !state.tasks.contains_key(&task.id))
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "工作流任务状态与当前计划不一致",
        ));
    }
    Ok(())
}

/// TDD evidence 裁决矩阵。
///
/// 全阶段强制：
/// - verification 必须非空，且每条 command+args ∈ task.verification；
/// - evidence ≤ 64 条、evidence.command ∈ task.verification、output ≤ 8192 字符；
/// - 顶层 message ≤ 2048 字符；filesChanged ≤ 500 条、每条 ≤ 512 字符；
/// - RED 额外要求：至少一条 verification.passed == false；
/// - 阶段矩阵：RED 需要预期失败证据，GREEN/REFACTOR 需要通过证据，VERIFY 需要全部通过。
pub(crate) fn validate_task_evidence(
    task: &TaskDefinition,
    parsed: &TaskExecutionResult,
) -> Result<(), SddError> {
    let verification = &parsed.verification;
    if verification.is_empty() {
        return Err(SddError::new(
            "E_TDD_EVIDENCE_REQUIRED",
            &format!(
                "任务 {} 缺少验证命令结果，请提供 verification 中各命令的执行结果",
                task.id
            ),
        ));
    }
    let mut submitted_commands = std::collections::BTreeSet::new();
    for item in verification {
        let rendered = std::iter::once(item.command.as_str())
            .chain(item.args.iter().map(String::as_str))
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !task.verification.iter().any(|allowed| allowed == &rendered) {
            return Err(SddError::new(
                "E_SECURITY_BLOCKED",
                &format!("任务结果包含未授权的验证命令：{rendered}"),
            ));
        }
        if !submitted_commands.insert(rendered) {
            return Err(SddError::new(
                "E_TDD_EVIDENCE_REQUIRED",
                "verification 不得重复提交同一命令",
            ));
        }
    }
    if submitted_commands.len() != task.verification.len()
        || task
            .verification
            .iter()
            .any(|command| !submitted_commands.contains(command))
    {
        return Err(SddError::new(
            "E_TDD_EVIDENCE_REQUIRED",
            &format!("任务 {} 必须提交全部计划验证命令的结果", task.id),
        ));
    }
    // RED 阶段额外要求：至少一条验证命令结果 passed=false（证明先看到失败）
    if task.phase == "RED" && !verification.iter().any(|item| !item.passed) {
        return Err(SddError::new(
            "E_TDD_EVIDENCE_REQUIRED",
            &format!(
                "任务 {}（RED）的验证结果必须包含 passed=false 的失败验证",
                task.id
            ),
        ));
    }
    for item in &parsed.evidence {
        if !task
            .verification
            .iter()
            .any(|allowed| allowed == &item.command)
        {
            return Err(SddError::new(
                "E_SECURITY_BLOCKED",
                &format!("任务结果包含未授权的证据命令：{}", item.command),
            ));
        }
    }
    let verification_passed = verification.iter().all(|item| item.passed);
    match task.phase.as_str() {
        "RED" => {
            if !parsed
                .evidence
                .iter()
                .any(|item| item.passed == Some(false) && item.expected_failure == Some(true))
            {
                return Err(SddError::new(
                    "E_TDD_EVIDENCE_REQUIRED",
                    &format!(
                        "任务 {}（RED）必须提供 passed=false 且 expectedFailure=true 的预期失败证据",
                        task.id
                    ),
                ));
            }
        }
        "GREEN" | "REFACTOR" => {
            if !parsed
                .evidence
                .iter()
                .any(|item| item.passed == Some(true) && item.expected_failure != Some(true))
            {
                return Err(SddError::new(
                    "E_TDD_EVIDENCE_REQUIRED",
                    &format!(
                        "任务 {}（{}）必须提供 passed=true 的通过证据",
                        task.id, task.phase
                    ),
                ));
            }
        }
        "VERIFY" if !verification_passed => {
            return Err(SddError::new(
                "E_VERIFY_FAILED",
                &format!("任务 {}（VERIFY）的完整验证未通过", task.id),
            ));
        }
        _ => {}
    }
    Ok(())
}

fn render_context_pack(
    cwd: &str,
    change_id: &str,
    task: &TaskDefinition,
    runtime: &crate::state::RuntimeDocument,
) -> Result<String, SddError> {
    let policies = crate::policies::builtin_build_policies();
    let policy_summary = policies
        .iter()
        .map(|policy| format!("- {}: {}", policy.name, policy.digest))
        .collect::<Vec<_>>()
        .join("\n");
    let change_dir = crate::state::paths::change_dir(cwd, change_id, false)?;
    let references = ["spec.md", "design.md", "plan.md", "tasks.md"]
        .iter()
        .map(|name| {
            let path = change_dir.join(name);
            crate::safe_fs::reject_symlink(&path, name)?;
            let source = fs::read_to_string(&path).map_err(|e| {
                SddError::new("E_MISSING_ARTIFACT", &format!("读取 {name} 失败：{e}"))
            })?;
            Ok(format!(
                "- .sdd/changes/{change_id}/{name}: {}",
                crate::policies::digest::digest(&source)
            ))
        })
        .collect::<Result<Vec<_>, SddError>>()?
        .join("\n");
    let mut codebase = runtime
        .index
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少代码库摘要"))?
        .replace(
            "END_UNTRUSTED_CODEBASE_CONTEXT",
            "ESCAPED_END_UNTRUSTED_CODEBASE_CONTEXT",
        );
    // 代码库上下文截断上限按 UTF-8 字节计算，避免多字节文本突破 KB 契约。
    let max_bytes = runtime
        .config
        .pointer("/contextPack/maxSizeKb")
        .and_then(|value| value.as_u64())
        .filter(|kb| *kb > 0)
        .and_then(|kb| usize::try_from(kb).ok())
        .and_then(|kb| kb.checked_mul(1024))
        .ok_or_else(|| {
            SddError::new(
                "E_STATE_CORRUPTED",
                "contextPack.maxSizeKb 必须是可表示的正整数",
            )
        })?;
    if codebase.len() > max_bytes {
        let mut boundary = max_bytes;
        while !codebase.is_char_boundary(boundary) {
            boundary -= 1;
        }
        codebase.truncate(boundary);
    }
    Ok(format!(
        "{}\n\n## References\n\n{}\n\nBEGIN_UNTRUSTED_CODEBASE_CONTEXT\n{}\nEND_UNTRUSTED_CODEBASE_CONTEXT\n\n## Policy Bundle\n\n{}",
        crate::engines::tdd::planner::render_context_pack(task),
        references,
        codebase,
        policy_summary
    ))
}

fn business_changes(cwd: &str) -> Result<Vec<String>, SddError> {
    GitInspector::business_changes(cwd)
}

fn task_changes(cwd: &str, pending: &serde_json::Value) -> Result<Vec<String>, SddError> {
    let baseline = pending
        .get("gitBaseline")
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "pendingAgentTask 缺少 gitBaseline"))?;
    if baseline
        .get("available")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "Git 仓库的 pendingAgentTask.gitBaseline 必须可用",
        ));
    }
    let baseline_files: std::collections::BTreeSet<String> = baseline
        .get("changedFiles")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            SddError::new(
                "E_STATE_CORRUPTED",
                "pendingAgentTask.gitBaseline 缺少 changedFiles",
            )
        })?
        .iter()
        .map(|value| {
            value.as_str().map(String::from).ok_or_else(|| {
                SddError::new(
                    "E_STATE_CORRUPTED",
                    "gitBaseline.changedFiles 必须是字符串数组",
                )
            })
        })
        .collect::<Result<_, _>>()?;
    let baseline_hashes: std::collections::BTreeMap<String, Option<String>> = baseline
        .get("changedFileHashes")
        .and_then(|value| value.as_object())
        .ok_or_else(|| {
            SddError::new(
                "E_STATE_CORRUPTED",
                "pendingAgentTask.gitBaseline 缺少 changedFileHashes",
            )
        })?
        .iter()
        .map(|(path, value)| {
            let hash = if value.is_null() {
                None
            } else {
                Some(value.as_str().map(String::from).ok_or_else(|| {
                    SddError::new(
                        "E_STATE_CORRUPTED",
                        "gitBaseline.changedFileHashes 的值必须是字符串或 null",
                    )
                })?)
            };
            Ok((path.clone(), hash))
        })
        .collect::<Result<_, SddError>>()?;
    GitInspector::changes_since(
        cwd,
        &baseline_files.into_iter().collect::<Vec<_>>(),
        &baseline_hashes,
    )
}

fn business_root(cwd: &str, state: &crate::state::WorkflowState) -> String {
    state
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.worktree_path.clone())
        .unwrap_or_else(|| cwd.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::validate::{TaskResultEvidence, TaskResultVerification};

    fn green_task() -> TaskDefinition {
        TaskDefinition {
            id: "TASK-001-GREEN".to_string(),
            title: "最小实现".to_string(),
            phase: "GREEN".to_string(),
            requirements: vec!["REQ-001".to_string()],
            scenarios: vec!["SCN-001".to_string()],
            depends_on: Vec::new(),
            allowed_files: vec!["src/lib.rs".to_string()],
            expected_new_files: Vec::new(),
            forbidden_files: vec![".sdd/**".to_string()],
            verification: vec!["cargo test".to_string(), "cargo clippy".to_string()],
            done_criteria: vec!["测试通过".to_string()],
            slice_type: "VERTICAL".to_string(),
            user_visible_outcome: "用户行为正确".to_string(),
            acceptance_criteria: vec!["SCN-001：用户行为正确".to_string()],
            test_seam: "src/lib.rs".to_string(),
        }
    }

    #[test]
    fn task_evidence_requires_each_planned_verification_exactly_once() {
        let mut result = TaskExecutionResult {
            task_id: "TASK-001-GREEN".to_string(),
            status: "completed".to_string(),
            evidence: vec![TaskResultEvidence {
                command: "cargo test".to_string(),
                passed: Some(true),
                expected_failure: None,
            }],
            verification: vec![TaskResultVerification {
                command: "cargo".to_string(),
                args: vec!["test".to_string()],
                passed: true,
            }],
            files_changed: Vec::new(),
        };

        assert_eq!(
            validate_task_evidence(&green_task(), &result)
                .unwrap_err()
                .code,
            "E_TDD_EVIDENCE_REQUIRED"
        );
        result.verification.push(TaskResultVerification {
            command: "cargo".to_string(),
            args: vec!["clippy".to_string()],
            passed: true,
        });
        assert!(validate_task_evidence(&green_task(), &result).is_ok());
        result.verification.push(result.verification[0].clone());
        assert_eq!(
            validate_task_evidence(&green_task(), &result)
                .unwrap_err()
                .code,
            "E_TDD_EVIDENCE_REQUIRED"
        );
    }
}
