//! build 命令：获取下一个任务（build next）或提交 Agent 结果（build complete）。
//!
//! 翻译自 Node 版 `packages/core/src/commands/build.ts` 的核心裁决语义：
//! - next：找下一个可执行任务，写 pendingAgentTask，返回 actionRequired
//! - complete：校验结果结构/任务身份/TDD evidence，写运行级结果并推进任务状态
//!
//! 契约变更点：actionRequired.codebase.provider 为
//! gitnexus | codegraph | fallback-file-scan（替代 codebase-memory-mcp）。

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::commands::plan::read_plan_tasks;
use crate::contracts::{
    AgentActionRequired, CodebaseProviderInfo, CommandResult, VerificationCommand,
};
use crate::engines::superpowers::protocol::{phase_instruction, TaskDefinition};
use crate::error::SddError;
use crate::git::GitInspector;
use crate::knowledge::provider::KnowledgeProvider;
use crate::knowledge::router::KnowledgeRouter;
use crate::state::file_lock::lock_sdd;
use crate::state::state_store::{
    TASK_STATUS_BUILDING, TASK_STATUS_DONE, TASK_STATUS_FAILED, TASK_STATUS_PENDING,
};
use crate::state::StateStore;

pub fn run_build(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    let args = args.cloned().unwrap_or(serde_json::Value::Null);
    let sub = args.get("sub").and_then(|v| v.as_str()).unwrap_or("next");
    match sub {
        "next" => run_build_next(cwd),
        "complete" => {
            let task_id = args.get("task").and_then(|v| v.as_str()).ok_or_else(|| {
                SddError::new("E_INVALID_PHASE_COMMAND", "build complete 需要 --task <id>")
            })?;
            let result_path = args.get("result").and_then(|v| v.as_str()).ok_or_else(|| {
                SddError::new(
                    "E_INVALID_PHASE_COMMAND",
                    "build complete 需要 --result <path>",
                )
            })?;
            run_build_complete(cwd, task_id, result_path)
        }
        _ => Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("未知 build 子命令：{sub}"),
        )),
    }
}

/// build next：返回下一个任务的 actionRequired
fn run_build_next(cwd: &str) -> Result<CommandResult, SddError> {
    let _guard = lock_sdd(cwd, "sdd build next", None, None)?;
    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;

    // 续跑：已有 pendingAgentTask 时直接返回
    if state.current_phase == "BUILD_WAITING_AGENT" && state.pending_agent_task.is_some() {
        if let Some(pending) = &state.pending_agent_task {
            let task_id = pending.get("taskId").and_then(|v| v.as_str()).unwrap_or("");
            let result_file = pending
                .get("resultFile")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let change_id = state.current_change_id.clone().unwrap_or_default();
            let task = find_task(cwd, &change_id, task_id)?;
            let context_pack = render_context_pack_path(cwd, task_id);
            return Ok(action_required_result(
                task,
                change_id,
                context_pack,
                result_file,
            ));
        }
    }
    if state.current_phase != "PLAN_READY" && state.current_phase != "BUILD_WAITING_AGENT" {
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("无法在 {} 状态下获取任务", state.current_phase),
        )
        .with_next("sdd plan"));
    }

    let change_id = current_change_id(&state)?;
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
            runtime_status == TASK_STATUS_PENDING
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

    let result_file = format!(
        ".sdd/runs/{}/tasks/{}.result.json",
        state.current_run_id.clone().unwrap_or_else(|| "run".into()),
        next_task.id
    );
    // 写 pendingAgentTask + 状态推进
    let git_baseline = match GitInspector::snapshot(cwd) {
        Ok(snapshot) => json!({
            "available": true,
            "head": snapshot.head,
            "files": snapshot.files,
        }),
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
            "resultFile": result_file,
            "since": crate::state::state_store::now_iso(),
            "gitBaseline": git_baseline,
        }));
        s.tasks
            .insert(next_task.id.clone(), TASK_STATUS_BUILDING.to_string());
    })?;

    let context_pack = render_context_pack_path(cwd, &next_task.id);
    Ok(action_required_result(
        next_task.clone(),
        change_id,
        context_pack,
        &result_file,
    ))
}

/// 构造 actionRequired 结果（契约变更：provider 来自 knowledge 路由）
fn action_required_result(
    task: TaskDefinition,
    change_id: String,
    context_pack: String,
    result_file: &str,
) -> CommandResult {
    // 探测当前可用知识图谱引擎（degraded 时取文件扫描）
    let router = KnowledgeRouter::new();
    let (provider, degraded) = {
        let g = router.gitnexus.probe();
        if g.available {
            ("gitnexus".to_string(), false)
        } else {
            let c = router.codegraph.probe();
            if c.available {
                ("codegraph".to_string(), false)
            } else {
                ("fallback-file-scan".to_string(), true)
            }
        }
    };
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
            result_file: result_file.to_string(),
            codebase: CodebaseProviderInfo { provider, degraded },
            policy_bundle: None,
        }),
        error: None,
    }
}

/// build complete：校验并推进任务
fn run_build_complete(
    cwd: &str,
    task_id: &str,
    result_path: &str,
) -> Result<CommandResult, SddError> {
    let _guard = lock_sdd(cwd, "sdd build complete", None, None)?;
    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
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

    // 读取并校验结果文件
    let result_path = PathBuf::from(cwd).join(result_path);
    let raw = fs::read_to_string(&result_path)
        .map_err(|e| SddError::new("E_AGENT_TASK_FAILED", &format!("读取任务结果失败：{e}")))?;
    let result: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        SddError::new(
            "E_TDD_EVIDENCE_REQUIRED",
            &format!("任务 {task_id} 的执行结果结构无效：{e}"),
        )
    })?;
    let result_task_id = result.get("taskId").and_then(|v| v.as_str()).unwrap_or("");
    if result_task_id != task_id {
        return Err(SddError::new(
            "E_AGENT_TASK_FAILED",
            &format!("结果文件中的 taskId（{result_task_id}）与请求的任务（{task_id}）不一致"),
        ));
    }
    let status = result
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("failed");
    if status != "completed" && status != "failed" {
        return Err(SddError::new(
            "E_TDD_EVIDENCE_REQUIRED",
            &format!("任务 {task_id} 的执行结果 status 非法：{status}"),
        ));
    }

    let change_id = current_change_id(&state)?;
    let tasks = read_plan_tasks(cwd, &change_id)?;
    let task = tasks.iter().find(|t| t.id == task_id).ok_or_else(|| {
        SddError::new("E_MISSING_ARTIFACT", &format!("计划中不存在任务 {task_id}"))
    })?;

    // TDD evidence 校验（RED 需要失败证据、GREEN 需要通过证据）
    validate_evidence(task, &result)?;

    // 写入运行级结果
    let run_id = state
        .current_run_id
        .clone()
        .unwrap_or_else(|| "run".to_string());
    let result_dir = PathBuf::from(cwd)
        .join(".sdd/runs")
        .join(&run_id)
        .join("tasks");
    fs::create_dir_all(&result_dir)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("创建结果目录失败：{e}")))?;
    fs::write(
        result_dir.join(format!("{task_id}.result.json")),
        serde_json::to_string_pretty(&result)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化结果失败：{e}")))?,
    )
    .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入结果失败：{e}")))?;

    // 推进任务状态：以运行时状态计算是否全部完成
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
            s.current_phase = "BUILD_READY".to_string();
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
        state: if status == "failed" {
            "FAILED".to_string()
        } else if all_done {
            "BUILD_READY".to_string()
        } else {
            "PLAN_READY".to_string()
        },
        exit_code: if status == "failed" { 7 } else { 0 },
        change_id: Some(change_id),
        next: Some(next.to_string()),
        data: Some(json!({ "taskId": task_id, "status": task_status })),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}

/// TDD evidence 校验（翻译自 tdd-evidence.ts 的裁决矩阵）
fn validate_evidence(task: &TaskDefinition, result: &serde_json::Value) -> Result<(), SddError> {
    let evidence = result
        .get("evidence")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let verification_passed = result
        .get("verification")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter().all(|item| {
                item.get("passed")
                    .and_then(|p| p.as_bool())
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    match task.phase.as_str() {
        "RED" => {
            if evidence == 0 {
                return Err(SddError::new(
                    "E_TDD_EVIDENCE_REQUIRED",
                    &format!("任务 {}（RED）必须提供测试失败的证据", task.id),
                ));
            }
        }
        "GREEN" | "REFACTOR" => {
            if evidence == 0 {
                return Err(SddError::new(
                    "E_TDD_EVIDENCE_REQUIRED",
                    &format!("任务 {}（{}）必须提供测试通过的证据", task.id, task.phase),
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

fn render_context_pack_path(cwd: &str, task_id: &str) -> String {
    // 上下文包按需生成：写 .sdd/context-packs/<task-id>/context.md
    let dir = PathBuf::from(cwd).join(".sdd/context-packs").join(task_id);
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(
        dir.join("context.md"),
        format!(
            "# Context Pack: {task_id}\n\n## TDD Instruction\n\n{}",
            phase_instruction("RED")
        ),
    );
    format!(".sdd/context-packs/{task_id}/context.md")
}
