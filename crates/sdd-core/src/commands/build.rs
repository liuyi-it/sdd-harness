//! build 命令：逐个派发纵向任务，并校验 Agent 回传的执行证据。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use serde_json::{json, Value};

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
    apply_workflow_update, ChangeWorkflow, TASK_STATUS_BUILDING, TASK_STATUS_DONE,
    TASK_STATUS_FAILED, TASK_STATUS_PENDING,
};

const MAX_RESULT_JSON_BYTES: usize = 4 * 1024 * 1024;

pub fn run_build(cwd: &str, args: Option<&Value>) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "changeId", "sub", "task", "resultJson"])?;
    let empty = Value::Null;
    let args = args.unwrap_or(&empty);
    let timeout_ms = super::timeout_ms(Some(args))?;
    match super::string_arg(Some(args), "sub")?.unwrap_or("next") {
        "next" => {
            if args.get("task").is_some() || args.get("resultJson").is_some() {
                return Err(SddError::new(
                    "E_INVALID_PHASE_COMMAND",
                    "build next 不接受 task 或 resultJson",
                ));
            }
            next(cwd, args, timeout_ms)
        }
        "complete" => {
            let task_id = super::string_arg(Some(args), "task")?.ok_or_else(|| {
                SddError::new("E_INVALID_PHASE_COMMAND", "build complete 需要 --task <id>")
            })?;
            let raw = super::string_arg(Some(args), "resultJson")?
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    SddError::new(
                        "E_INVALID_PHASE_COMMAND",
                        "build complete 需要 --result-json '<JSON>'",
                    )
                })?;
            if raw.len() > MAX_RESULT_JSON_BYTES {
                return Err(SddError::new(
                    "E_TDD_EVIDENCE_REQUIRED",
                    &format!("resultJson 超过 {MAX_RESULT_JSON_BYTES} 字节上限"),
                ));
            }
            complete(cwd, args, task_id, raw, timeout_ms)
        }
        sub => Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("未知 build 子命令：{sub}"),
        )),
    }
}

fn next(cwd: &str, args: &Value, timeout_ms: Option<u64>) -> Result<CommandResult, SddError> {
    let _guard = lock_initialized_sdd(cwd, timeout_ms)?;
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let change_id = super::resolve_change_id(&runtime, Some(args))?;
    let workflow = super::workflow(&runtime, &change_id)?;
    super::ensure_phase(workflow, "build", &change_id)?;
    verify_plan_artifacts(cwd, &runtime, &change_id)?;
    let tasks = read_tasks(&runtime, &change_id)?;
    validate_runtime_task_state(workflow, &tasks)?;

    if workflow.phase == "BUILD_WAITING_AGENT" {
        let pending = pending(workflow)?;
        let task_id = pending_task_id(pending)?;
        let task = find_task(&tasks, task_id)?.clone();
        return action(cwd, &runtime, &change_id, task);
    }
    if workflow.phase == "BUILD_READY" {
        return Err(
            SddError::new("E_INVALID_PHASE_COMMAND", "所有计划任务均已完成")
                .with_next(&format!("sdd verify --change {change_id}")),
        );
    }

    let task = tasks
        .iter()
        .find(|task| {
            matches!(
                workflow.tasks.get(&task.id).map(String::as_str),
                Some(TASK_STATUS_PENDING | TASK_STATUS_FAILED)
            ) && task.depends_on.iter().all(|dependency| {
                workflow.tasks.get(dependency).map(String::as_str) == Some(TASK_STATUS_DONE)
            })
        })
        .cloned()
        .ok_or_else(|| {
            SddError::new(
                "E_STATE_CORRUPTED",
                "没有可执行任务：任务依赖或运行状态不一致",
            )
        })?;
    validate_task_verification(&task)?;
    let business_cwd = business_root(cwd, workflow);
    let git_baseline = if GitInspector::is_git_repo(&business_cwd)? {
        let changed_files = GitInspector::business_changes(&business_cwd)?;
        json!({
            "available": true,
            "head": GitInspector::head(&business_cwd)?,
            "changedFiles": changed_files,
            "changedFileHashes": GitInspector::file_hashes(&business_cwd, &changed_files)?,
        })
    } else {
        json!({ "available": false })
    };
    let pending = json!({
        "taskId": task.id,
        "since": crate::state::state_store::now_iso(),
        "gitBaseline": git_baseline,
    });
    let task_id = task.id.clone();
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        let workflow = super::workflow_mut(document, &change_id)?;
        apply_workflow_update(workflow, |workflow| {
            workflow.phase = "BUILD_WAITING_AGENT".to_string();
            workflow.in_progress_phase = Some("BUILDING".to_string());
            workflow.pending_agent_action = Some(pending.clone());
            workflow
                .tasks
                .insert(task_id.clone(), TASK_STATUS_BUILDING.to_string());
            workflow.suggested_command = Some(format!(
                "sdd build complete --change {change_id} --task {task_id} --result-json '<JSON>'"
            ));
            workflow.last_command = Some("sdd build next".to_string());
            workflow.clear_failure();
        })
    })?;
    let current = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    action(cwd, &current, &change_id, task)
}

fn complete(
    cwd: &str,
    args: &Value,
    task_id: &str,
    raw: &str,
    timeout_ms: Option<u64>,
) -> Result<CommandResult, SddError> {
    let _guard = lock_initialized_sdd(cwd, timeout_ms)?;
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let change_id = super::resolve_change_id(&runtime, Some(args))?;
    let workflow = super::workflow(&runtime, &change_id)?;
    super::ensure_phase(workflow, "build", &change_id)?;
    if workflow.phase != "BUILD_WAITING_AGENT" {
        return Err(
            SddError::new("E_INVALID_PHASE_COMMAND", "当前没有等待提交的构建任务")
                .with_next(&format!("sdd build next --change {change_id}")),
        );
    }
    let pending = pending(workflow)?;
    let expected_task_id = pending_task_id(pending)?;
    if expected_task_id != task_id {
        return Err(SddError::new(
            "E_AGENT_TASK_FAILED",
            &format!("当前等待任务是 {expected_task_id}，不是 {task_id}"),
        ));
    }

    let result: Value = serde_json::from_str(raw).map_err(|error| {
        SddError::new(
            "E_TDD_EVIDENCE_REQUIRED",
            &format!("任务结果不是合法 JSON：{error}"),
        )
    })?;
    let parsed = validate_task_result(&result)?;
    if parsed.task_id != task_id {
        return Err(SddError::new(
            "E_AGENT_TASK_FAILED",
            "resultJson.taskId 与当前任务不一致",
        ));
    }
    let tasks = read_tasks(&runtime, &change_id)?;
    validate_runtime_task_state(workflow, &tasks)?;
    let task = find_task(&tasks, task_id)?;
    validate_task_evidence(task, &parsed)?;

    let business_cwd = business_root(cwd, workflow);
    for path in &parsed.files_changed {
        GitInspector::resolve_repo_path(&business_cwd, path)?;
    }
    let mut warnings = Vec::new();
    let actual_files = if GitInspector::is_git_repo(&business_cwd)? {
        let mut actual = task_changes(&business_cwd, pending)?;
        let mut declared = parsed.files_changed.clone();
        actual.sort();
        actual.dedup();
        declared.sort();
        declared.dedup();
        if actual != declared {
            return Err(SddError::new(
                "E_UNDECLARED_FILE_CHANGE",
                &format!(
                    "filesChanged 与 Git 事实不一致（声明：{}；实际：{}）",
                    declared.join("、"),
                    actual.join("、")
                ),
            ));
        }
        actual
    } else {
        warnings.push(CliWarning::new(
            "W_NO_GIT_FACTS",
            "当前目录不是 Git 仓库，只能校验 Agent 声明的文件范围",
        ));
        parsed.files_changed.clone()
    };
    validate_file_change(
        &actual_files,
        &task.allowed_files,
        &task.expected_new_files,
        &task.forbidden_files,
    )?;

    let completed = parsed.status == "completed";
    let all_done = tasks.iter().all(|task| {
        if task.id == task_id {
            completed
        } else {
            workflow.tasks.get(&task.id).map(String::as_str) == Some(TASK_STATUS_DONE)
        }
    });
    let run_id = workflow.run_id.clone();
    let artifact_key = format!("{run_id}:{task_id}:result");
    let content_path = format!("runtime://runs/{run_id}/tasks/{task_id}");
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        document
            .runs
            .get_mut(&run_id)
            .and_then(Value::as_object_mut)
            .and_then(|run| run.get_mut("tasks"))
            .and_then(Value::as_object_mut)
            .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "run.tasks 必须是对象"))?
            .insert(task_id.to_string(), result.clone());
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
        let workflow = super::workflow_mut(document, &change_id)?;
        apply_workflow_update(workflow, |workflow| {
            workflow.tasks.insert(
                task_id.to_string(),
                if completed {
                    TASK_STATUS_DONE
                } else {
                    TASK_STATUS_FAILED
                }
                .to_string(),
            );
            workflow.pending_agent_action = None;
            workflow.in_progress_phase = None;
            workflow.last_command = Some("sdd build complete".to_string());
            if !completed {
                workflow.phase = "PLAN_READY".to_string();
                workflow.record_failure("sdd build complete", format!("任务 {task_id} 执行失败"));
                workflow.suggested_command = Some(format!("sdd build next --change {change_id}"));
            } else if all_done {
                workflow.phase = "BUILD_READY".to_string();
                workflow.clear_failure();
                workflow.suggested_command = Some(format!("sdd verify --change {change_id}"));
            } else {
                workflow.phase = "PLAN_READY".to_string();
                workflow.clear_failure();
                workflow.suggested_command = Some(format!("sdd build next --change {change_id}"));
            }
        })
    })?;

    let phase = if all_done {
        "BUILD_READY"
    } else {
        "PLAN_READY"
    };
    let next = if all_done {
        format!("sdd verify --change {change_id}")
    } else {
        format!("sdd build next --change {change_id}")
    };
    Ok(CommandResult {
        ok: completed,
        state: phase.to_string(),
        exit_code: if completed { 0 } else { 7 },
        change_id: Some(change_id),
        next: Some(next),
        data: Some(json!({
            "taskId": task_id,
            "status": if completed { TASK_STATUS_DONE } else { TASK_STATUS_FAILED },
        })),
        rendered: None,
        warnings: (!warnings.is_empty()).then_some(warnings),
        action_required: None,
        error: None,
    })
}

fn action(
    cwd: &str,
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
    task: TaskDefinition,
) -> Result<CommandResult, SddError> {
    validate_task_verification(&task)?;
    let context_pack = render_context_pack(cwd, change_id, &task, runtime)?;
    let verification = task
        .verification
        .iter()
        .map(|item| VerificationCommand {
            command: item.command.clone(),
            args: item.args.clone(),
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
    Ok(CommandResult {
        ok: true,
        state: "BUILD_WAITING_AGENT".to_string(),
        exit_code: 0,
        change_id: Some(change_id.to_string()),
        next: Some(format!(
            "sdd build complete --change {change_id} --task {} --result-json '<JSON>'",
            task.id
        )),
        data: None,
        rendered: None,
        warnings: None,
        action_required: Some(AgentActionRequired::AgentTaskExecution {
            task_id: task.id,
            change_id: change_id.to_string(),
            context_pack,
            allowed_files: task.allowed_files,
            expected_new_files: task.expected_new_files,
            forbidden_files: task.forbidden_files,
            verification,
            result_schema: crate::schema::schema_value("task-result")?.clone(),
            result_transport: "inline-json".to_string(),
            codebase: CodebaseProviderInfo {
                provider: runtime.state.codebase_provider.clone(),
                degraded: runtime.state.degraded,
            },
            policy_bundle: Some(policy_bundle),
        }),
        error: None,
    })
}

fn validate_task_verification(task: &TaskDefinition) -> Result<(), SddError> {
    for verification in &task.verification {
        crate::security::verification_command::validate_verification_command(
            &verification.command,
            &verification.args,
        )?;
    }
    Ok(())
}

pub(crate) fn validate_runtime_task_state(
    workflow: &ChangeWorkflow,
    tasks: &[TaskDefinition],
) -> Result<(), SddError> {
    if workflow.tasks.len() != tasks.len()
        || tasks
            .iter()
            .any(|task| !workflow.tasks.contains_key(&task.id))
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "change 的任务状态与当前计划不一致",
        ));
    }
    Ok(())
}

pub(crate) fn validate_task_evidence(
    task: &TaskDefinition,
    parsed: &TaskExecutionResult,
) -> Result<(), SddError> {
    let planned = task
        .verification
        .iter()
        .map(|item| (&item.command, &item.args))
        .collect::<BTreeSet<_>>();
    let mut submitted = BTreeSet::new();
    for result in &parsed.verification {
        let command = verification_text(&result.command, &result.args);
        let invocation = (&result.command, &result.args);
        if !planned.contains(&invocation) {
            return Err(SddError::new(
                "E_SECURITY_BLOCKED",
                &format!("任务结果包含未授权验证命令：{command}"),
            ));
        }
        if !submitted.insert(invocation) {
            return Err(SddError::new(
                "E_TDD_EVIDENCE_REQUIRED",
                "同一验证命令不得重复提交",
            ));
        }
    }
    if submitted != planned {
        return Err(SddError::new(
            "E_TDD_EVIDENCE_REQUIRED",
            &format!("任务 {} 必须提交全部计划验证命令", task.id),
        ));
    }
    if parsed.status == "completed" && parsed.verification.iter().any(|item| !item.passed) {
        return Err(SddError::new(
            "E_VERIFY_FAILED",
            &format!("任务 {} 声明完成，但仍有验证失败", task.id),
        ));
    }
    let planned_text = task
        .verification
        .iter()
        .map(|item| verification_text(&item.command, &item.args))
        .collect::<BTreeSet<_>>();
    for evidence in &parsed.evidence {
        if !planned_text.contains(&evidence.command) {
            return Err(SddError::new(
                "E_SECURITY_BLOCKED",
                &format!("任务证据包含未授权命令：{}", evidence.command),
            ));
        }
    }
    if task.execution_mode == "TDD" && parsed.status == "completed" {
        let expected_failure = parsed
            .evidence
            .iter()
            .any(|item| item.passed == Some(false) && item.expected_failure == Some(true));
        let passing = parsed
            .evidence
            .iter()
            .any(|item| item.passed == Some(true) && item.expected_failure != Some(true));
        if !expected_failure || !passing {
            return Err(SddError::new(
                "E_TDD_EVIDENCE_REQUIRED",
                &format!("TDD 任务 {} 必须同时提供预期失败和最终通过证据", task.id),
            ));
        }
    }
    Ok(())
}

fn verification_text(command: &str, args: &[String]) -> String {
    std::iter::once(command)
        .chain(args.iter().map(String::as_str))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_context_pack(
    cwd: &str,
    change_id: &str,
    task: &TaskDefinition,
    runtime: &crate::state::RuntimeDocument,
) -> Result<String, SddError> {
    let change_dir = crate::state::paths::change_dir(cwd, change_id, false)?;
    let documents = ["spec.md", "plan.md", "tasks.md"]
        .iter()
        .map(|name| {
            let path = change_dir.join(name);
            crate::safe_fs::reject_symlink(&path, name)?;
            fs::read_to_string(path)
                .map(|content| format!("## {name}\n\n{content}"))
                .map_err(|error| {
                    SddError::new("E_MISSING_ARTIFACT", &format!("读取 {name} 失败：{error}"))
                })
        })
        .collect::<Result<Vec<_>, _>>()?
        .join("\n\n");
    let task_json = serde_json::to_string_pretty(task)
        .map_err(|error| SddError::new("E_STATE_CORRUPTED", &error.to_string()))?;
    let codebase = runtime
        .index
        .get("summary")
        .and_then(Value::as_str)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime 缺少代码库摘要"))?
        .replace(
            "END_UNTRUSTED_CODEBASE_CONTEXT",
            "ESCAPED_END_UNTRUSTED_CODEBASE_CONTEXT",
        );
    Ok(format!(
        "# 纵向实施任务\n\n{task_json}\n\n# 已批准文档\n\n{documents}\n\nBEGIN_UNTRUSTED_CODEBASE_CONTEXT\n{codebase}\nEND_UNTRUSTED_CODEBASE_CONTEXT\n\n只修改 allowedFiles；按 steps 在一个任务内完成测试、实现和最终验证，并通过 inline JSON 回传全部证据。"
    ))
}

fn verify_plan_artifacts(
    cwd: &str,
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
) -> Result<(), SddError> {
    crate::state::artifact_store::verify_artifacts_in(
        cwd,
        runtime,
        [
            format!("{change_id}:spec"),
            format!("{change_id}:plan"),
            format!("{change_id}:plan-md"),
            format!("{change_id}:tasks-md"),
        ],
    )
}

fn read_tasks(
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
) -> Result<Vec<TaskDefinition>, SddError> {
    let plan = runtime
        .changes
        .get(change_id)
        .and_then(|change| change.get("plan"))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime 缺少 plan"))?;
    plan_tasks(plan)
}

fn find_task<'a>(
    tasks: &'a [TaskDefinition],
    task_id: &str,
) -> Result<&'a TaskDefinition, SddError> {
    tasks
        .iter()
        .find(|task| task.id == task_id)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", &format!("计划中不存在任务 {task_id}")))
}

fn pending(workflow: &ChangeWorkflow) -> Result<&Value, SddError> {
    workflow.pending_agent_action.as_ref().ok_or_else(|| {
        SddError::new(
            "E_STATE_CORRUPTED",
            "BUILD_WAITING_AGENT 缺少 pendingAgentAction",
        )
    })
}

fn pending_task_id(pending: &Value) -> Result<&str, SddError> {
    pending
        .get("taskId")
        .and_then(Value::as_str)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "pendingAgentAction 缺少 taskId"))
}

fn business_root(cwd: &str, workflow: &ChangeWorkflow) -> String {
    workflow
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.worktree_path.clone())
        .unwrap_or_else(|| cwd.to_string())
}

fn task_changes(cwd: &str, pending: &Value) -> Result<Vec<String>, SddError> {
    let baseline = pending
        .get("gitBaseline")
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "任务缺少 Git 基线"))?;
    if baseline.get("available").and_then(Value::as_bool) != Some(true) {
        return Err(SddError::new("E_STATE_CORRUPTED", "任务 Git 基线不可用"));
    }
    let baseline_files = baseline
        .get("changedFiles")
        .and_then(Value::as_array)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "Git 基线缺少 changedFiles"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(String::from)
                .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "changedFiles 必须是字符串"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let baseline_hashes = baseline
        .get("changedFileHashes")
        .and_then(Value::as_object)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "Git 基线缺少文件哈希"))?
        .iter()
        .map(|(path, value)| {
            let hash =
                if value.is_null() {
                    None
                } else {
                    Some(value.as_str().map(String::from).ok_or_else(|| {
                        SddError::new("E_STATE_CORRUPTED", "文件哈希必须是字符串")
                    })?)
                };
            Ok((path.clone(), hash))
        })
        .collect::<Result<BTreeMap<_, _>, SddError>>()?;
    GitInspector::changes_since(cwd, &baseline_files, &baseline_hashes)
}
