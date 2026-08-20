//! build 命令：获取下一个任务（build next）或提交 Agent 结果（build complete）。
//!
//! 翻译自 早期 Node 实现 的核心裁决语义：
//! - next：找下一个可执行任务，写 pendingAgentTask，返回 actionRequired
//! - complete：校验结果结构/任务身份/TDD evidence，写运行级结果并推进任务状态
//!
//! 契约变更点：actionRequired.codebase.provider 为
//! codegraph | fallback-file-scan。

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::commands::plan::read_plan_tasks;
use crate::contracts::{
    AgentActionRequired, CodebaseProviderInfo, CommandResult, VerificationCommand,
};
use crate::engines::superpowers::protocol::TaskDefinition;
use crate::error::SddError;
use crate::git::GitInspector;
use crate::protocol::{validate_task_result, TaskExecutionResult};
use crate::security::task_scope::validate_file_change;
use crate::state::file_lock::lock_sdd;
use crate::state::state_store::{
    TASK_STATUS_BUILDING, TASK_STATUS_DONE, TASK_STATUS_FAILED, TASK_STATUS_PENDING,
};
use crate::state::StateStore;

pub fn run_build(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    let args = args.cloned().unwrap_or(serde_json::Value::Null);
    let timeout_ms = args
        .get("timeout")
        .and_then(|value| value.as_f64())
        .map(|seconds| (seconds * 1000.0) as u64);
    let sub = args.get("sub").and_then(|v| v.as_str()).unwrap_or("next");
    match sub {
        "next" => run_build_next(cwd, timeout_ms),
        "complete" => {
            let task_id = args.get("task").and_then(|v| v.as_str()).ok_or_else(|| {
                SddError::new("E_INVALID_PHASE_COMMAND", "build complete 需要 --task <id>")
            })?;
            let result_json = if let Some(result_json) =
                args.get("resultJson").and_then(|v| v.as_str())
            {
                result_json.to_string()
            } else if let Some(result_path) = args.get("resultPath").and_then(|v| v.as_str()) {
                fs::read_to_string(result_path).map_err(|error| {
                    SddError::new(
                        "E_MISSING_ARTIFACT",
                        &format!("读取任务结果文件失败：{error}"),
                    )
                })?
            } else {
                return Err(SddError::new(
                    "E_INVALID_PHASE_COMMAND",
                    "build complete 需要 --result <path> 或 --result-json '<TaskExecutionResult JSON>'",
                ));
            };
            run_build_complete(cwd, task_id, &result_json, timeout_ms)
        }
        _ => Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("未知 build 子命令：{sub}"),
        )),
    }
}

/// build next：返回下一个任务的 actionRequired
fn run_build_next(cwd: &str, timeout_ms: Option<u64>) -> Result<CommandResult, SddError> {
    let _guard = lock_sdd(cwd, "sdd build next", None, timeout_ms)?;
    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    let business_cwd = business_root(cwd, &state);
    let change_id = current_change_id(&state)?;
    crate::state::artifact_store::verify_artifact(cwd, &format!("{change_id}:plan"))?;

    // 续跑：已有 pendingAgentTask 时直接返回
    if state.current_phase == "BUILD_WAITING_AGENT" && state.pending_agent_task.is_some() {
        if let Some(pending) = &state.pending_agent_task {
            let task_id = pending.get("taskId").and_then(|v| v.as_str()).unwrap_or("");
            let result_transport = pending
                .get("resultTransport")
                .and_then(|v| v.as_str())
                .unwrap_or("inline-json");
            let task = find_task(cwd, &change_id, task_id)?;
            validate_task_verification(&task)?;
            let context_pack = render_context_pack(cwd, &task)?;
            return Ok(action_required_result(
                task,
                change_id,
                context_pack,
                result_transport,
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

    let tasks = read_plan_tasks(cwd, &change_id)?;

    // 找下一个可执行任务：以运行时状态（state.tasks）为准，
    // 所有依赖 DONE、自身 PENDING
    let next_task = tasks
        .iter()
        .find(|task| {
            let runtime_status = state
                .tasks
                .get(&task.id)
                .map(|s| s.as_str())
                .unwrap_or(task.status.as_str());
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
    let result_transport = "inline-json";
    // 写 pendingAgentTask + 状态推进
    let git_baseline = match GitInspector::snapshot(&business_cwd) {
        Ok(snapshot) => {
            let changed_files = business_changes(&business_cwd)?;
            json!({
                "available": true,
                "head": snapshot.head,
                "files": snapshot.files,
                "changedFiles": changed_files,
                "changedFileHashes": GitInspector::file_hashes(&business_cwd, &changed_files)?,
            })
        }
        Err(_) => json!({ "available": false }),
    };
    store.update(|s| {
        s.current_phase = "BUILD_WAITING_AGENT".to_string();
        s.in_progress_phase = Some("BUILDING".to_string());
        s.suggested_command = Some("sdd build complete".to_string());
        s.last_command = Some("sdd build next".to_string());
        s.last_error = None;
        s.pending_agent_task = Some(json!({
            "taskId": next_task.id,
            "resultTransport": result_transport,
            "since": crate::state::state_store::now_iso(),
            "gitBaseline": git_baseline,
        }));
        s.tasks
            .insert(next_task.id.clone(), TASK_STATUS_BUILDING.to_string());
    })?;

    let context_pack = render_context_pack(cwd, next_task)?;
    if let Some(run_id) = state.current_run_id.as_deref() {
        crate::state::runtime_store::write_run_field(
            cwd,
            run_id,
            &format!("contexts/{}", next_task.id),
            json!(context_pack),
        )?;
    }
    Ok(action_required_result(
        next_task.clone(),
        change_id,
        context_pack,
        result_transport,
        state.codebase_provider,
        state.degraded,
    ))
}

/// 构造 actionRequired 结果（契约变更：provider 来自 knowledge 路由）
/// 构造 actionRequired 结果（结果通过 inline JSON 提交）。
fn action_required_result(
    task: TaskDefinition,
    change_id: String,
    context_pack: String,
    result_transport: &str,
    provider: String,
    degraded: bool,
) -> CommandResult {
    let verification: Vec<VerificationCommand> = task
        .verification
        .iter()
        .map(|cmd| {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            VerificationCommand {
                command: parts.first().unwrap_or(&"").to_string(),
                args: parts[1..].iter().map(|s| s.to_string()).collect(),
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
            task_id: task.id.clone(),
            change_id,
            context_pack,
            allowed_files: task.allowed_files.clone(),
            expected_new_files: task.expected_new_files.clone(),
            forbidden_files: task.forbidden_files.clone(),
            verification,
            result_transport: result_transport.to_string(),
            codebase: CodebaseProviderInfo { provider, degraded },
            policy_bundle: Some(policy_bundle),
        }),
        error: None,
    }
}

/// build complete：校验并推进任务
fn run_build_complete(
    cwd: &str,
    task_id: &str,
    result_json: &str,
    timeout_ms: Option<u64>,
) -> Result<CommandResult, SddError> {
    let _guard = lock_sdd(cwd, "sdd build complete", None, timeout_ms)?;
    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    let business_cwd = business_root(cwd, &state);
    let change_id = current_change_id(&state)?;
    crate::state::artifact_store::verify_artifact(cwd, &format!("{change_id}:plan"))?;
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
    let mut warnings: Vec<serde_json::Value> = Vec::new();
    let parsed = validate_task_result(&result)?;
    if parsed.task_id != task_id {
        return Err(SddError::new(
            "E_AGENT_TASK_FAILED",
            &format!(
                "结果文件中的 taskId（{}）与请求的任务（{task_id}）不一致",
                parsed.task_id
            ),
        ));
    }
    let status = parsed.status.as_str();

    let tasks = read_plan_tasks(cwd, &change_id)?;
    let task = tasks.iter().find(|t| t.id == task_id).ok_or_else(|| {
        SddError::new("E_MISSING_ARTIFACT", &format!("计划中不存在任务 {task_id}"))
    })?;

    // TDD evidence 校验（RED 需要失败证据、GREEN 需要通过证据）
    validate_task_evidence(task, &parsed)?;

    for path in &parsed.files_changed {
        GitInspector::resolve_repo_path(&business_cwd, path)?;
    }
    let actual_files = if GitInspector::is_git_repo(&business_cwd) {
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
            warnings.push(json!({
                "code": "W_NO_GIT_FACTS",
                "message": "当前目录不是 git 仓库，无法用 Git 事实核对 filesChanged 声明",
            }));
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
    let run_id = state
        .current_run_id
        .clone()
        .unwrap_or_else(|| "run".to_string());
    let result_text = serde_json::to_string_pretty(&result)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化结果失败：{e}")))?;
    let mut run_tasks = crate::state::runtime_store::read_run_field(cwd, &run_id, "tasks")?
        .unwrap_or_else(|| json!({}));
    if !run_tasks.is_object() {
        run_tasks = json!({});
    }
    run_tasks[task_id] = result;
    crate::state::runtime_store::write_run_field(cwd, &run_id, "tasks", run_tasks)?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{run_id}:{task_id}:result"),
        "report",
        &format!("runtime://runs/{run_id}/tasks/{task_id}"),
        &result_text,
        json!({ "taskId": task_id }),
    )?;

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
    store.update(|s| {
        s.tasks.insert(task_id.to_string(), task_status.to_string());
        s.pending_agent_task = None;
        if status == "failed" {
            // 任务失败：回到 PLAN_READY，FAILED 任务可由 build next 重新派发
            s.current_phase = "PLAN_READY".to_string();
            s.failed_command = Some("sdd build complete".to_string());
            s.failed_reason = Some(format!("任务 {task_id} 执行失败"));
            s.suggested_command = Some("sdd build next".to_string());
        } else if all_done {
            s.current_phase = "BUILD_READY".to_string();
            s.in_progress_phase = None;
            s.suggested_command = Some("sdd verify".to_string());
        } else {
            s.current_phase = "PLAN_READY".to_string();
            s.suggested_command = Some("sdd build next".to_string());
        }
        s.last_command = Some("sdd build complete".to_string());
        s.last_error = None;
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

/// TDD evidence 校验（翻译自 tdd-evidence.ts 的裁决矩阵）
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
    }
    // RED 阶段额外要求：至少一条验证命令结果 passed=false（证明先看到失败）
    if task.phase == "RED" && !verification.iter().any(|item| item.passed == Some(false)) {
        return Err(SddError::new(
            "E_TDD_EVIDENCE_REQUIRED",
            &format!(
                "任务 {}（RED）的验证结果必须包含 passed=false 的失败验证",
                task.id
            ),
        ));
    }
    // evidence 数组上限与字段约束
    if parsed.evidence.len() > 64 {
        return Err(SddError::new(
            "E_TDD_EVIDENCE_REQUIRED",
            &format!("任务 {} 的 evidence 超过 64 条上限", task.id),
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
        if item.output.chars().count() > 8192 {
            return Err(SddError::new(
                "E_TDD_EVIDENCE_REQUIRED",
                &format!("任务 {} 的 evidence.output 超过 8192 字符", task.id),
            ));
        }
    }
    if let Some(message) = &parsed.message {
        if message.chars().count() > 2048 {
            return Err(SddError::new(
                "E_TDD_EVIDENCE_REQUIRED",
                &format!("任务 {} 的 message 超过 2048 字符", task.id),
            ));
        }
    }
    if parsed.files_changed.len() > 500 {
        return Err(SddError::new(
            "E_TDD_EVIDENCE_REQUIRED",
            &format!("任务 {} 的 filesChanged 超过 500 条上限", task.id),
        ));
    }
    for path in &parsed.files_changed {
        if path.chars().count() > 512 {
            return Err(SddError::new(
                "E_TDD_EVIDENCE_REQUIRED",
                &format!("任务 {} 的 filesChanged 路径超过 512 字符：{path}", task.id),
            ));
        }
    }
    let verification_passed = verification.iter().all(|item| item.passed.unwrap_or(false));
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

fn find_task(cwd: &str, change_id: &str, task_id: &str) -> Result<TaskDefinition, SddError> {
    let tasks = read_plan_tasks(cwd, change_id)?;
    tasks
        .into_iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", &format!("计划中不存在任务 {task_id}")))
}

fn render_context_pack(cwd: &str, task: &TaskDefinition) -> Result<String, SddError> {
    let policies = crate::policies::builtin_build_policies();
    let policy_summary = policies
        .iter()
        .map(|policy| format!("- {}: {}", policy.name, policy.digest))
        .collect::<Vec<_>>()
        .join("\n");
    let state = StateStore::new(cwd.to_string()).read()?;
    let change_id = current_change_id(&state)?;
    let change_dir = PathBuf::from(cwd).join(".sdd/changes").join(&change_id);
    let references = ["spec.md", "design.md", "plan.md", "tasks.md"]
        .iter()
        .map(|name| {
            let path = change_dir.join(name);
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
    let codebase = crate::state::runtime_store::read_index_field(cwd, "summary")?
        .and_then(|value| value.as_str().map(String::from))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少代码库摘要"))?
        .replace(
            "END_UNTRUSTED_CODEBASE_CONTEXT",
            "ESCAPED_END_UNTRUSTED_CODEBASE_CONTEXT",
        );
    // 代码库上下文截断上限：config 的 contextPack.maxSizeKb（默认 30，单位 KB）
    let config = crate::state::runtime_store::read_config(cwd)?;
    let max_chars = config
        .pointer("/contextPack/maxSizeKb")
        .and_then(|value| value.as_u64())
        .filter(|kb| *kb > 0)
        .map(|kb| (kb * 1024) as usize)
        .unwrap_or(30 * 1024);
    let codebase = codebase.chars().take(max_chars).collect::<String>();
    Ok(format!(
        "{}\n\n## References\n\n{}\n\nBEGIN_UNTRUSTED_CODEBASE_CONTEXT\n{}\nEND_UNTRUSTED_CODEBASE_CONTEXT\n\n## Policy Bundle\n\n{}",
        crate::engines::superpowers::planner::render_context_pack(task),
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
    let baseline_files: std::collections::BTreeSet<String> = baseline
        .get("changedFiles")
        .and_then(|value| value.as_array())
        .map(|files| {
            files
                .iter()
                .filter_map(|value| value.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let baseline_hashes: std::collections::BTreeMap<String, Option<String>> = baseline
        .get("changedFileHashes")
        .and_then(|value| value.as_object())
        .map(|hashes| {
            hashes
                .iter()
                .map(|(path, value)| (path.clone(), value.as_str().map(String::from)))
                .collect()
        })
        .unwrap_or_default();
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
