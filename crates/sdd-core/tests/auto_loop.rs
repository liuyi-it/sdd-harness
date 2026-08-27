//! auto 命令测试。

use sdd_core::contracts::CommandRequest;
use sdd_core::run;
use serde_json::json;

const FULL_REQUIREMENT: &str = "授权用户通过 POST /orders/{id}/cancel 请求取消待处理订单，入参 order_id，返回 status 和 error_code，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";

fn setup(dir: &std::path::Path) -> String {
    std::fs::write(dir.join("README.md"), "# demo").unwrap();
    let cwd = dir.to_string_lossy().to_string();
    run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    sdd_core::state::RuntimeStore::new(cwd.clone())
        .update(|runtime| {
            let prefix = if runtime.state.degraded {
                "<!-- summary-provider: fallback-file-scan degraded=true -->"
            } else {
                "<!-- summary-provider: codegraph degraded=false -->"
            };
            runtime.index["summary"] = json!(format!(
                "{prefix}\nsrc/order_service.rs\nsrc/order_service.test.rs\nCargo.toml\n"
            ));
            runtime.index["updatedAt"] = json!("2026-01-01T00:00:00Z");
        })
        .unwrap();
    cwd
}

#[test]
fn auto_pauses_on_missing_requirement() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    let result = run(&CommandRequest {
        command: "auto".into(),
        cwd,
        args: None,
    })
    .unwrap();
    assert!(!result.ok);
    assert_eq!(result.state, "INDEX_READY");
    assert!(result.data.is_some());
}

#[test]
fn auto_runs_to_agent_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    let result = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    // 确定性步骤推进到 build 后暂停等待 Agent
    assert_eq!(result.state, "BUILD_WAITING_AGENT");
    assert!(result.action_required.is_some());
    // 状态已推进
    let status = run(&CommandRequest {
        command: "status".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert_eq!(status.state, "BUILD_WAITING_AGENT");
    let loop_status = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "loopStatus": true })),
    })
    .unwrap();
    assert_eq!(
        loop_status.data.unwrap()["activeLoop"]["status"],
        "WAITING_AGENT"
    );

    let events = run(&CommandRequest {
        command: "auto".into(),
        cwd,
        args: Some(json!({ "events": true, "tail": 2 })),
    })
    .unwrap();
    assert_eq!(events.data.unwrap()["events"].as_array().unwrap().len(), 2);
}

#[test]
fn auto_stop_keeps_workflow_phase_and_resume_restores_loop() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    let stopped = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "stop": true })),
    })
    .unwrap();
    assert_eq!(stopped.state, "BUILD_WAITING_AGENT");
    assert_eq!(
        sdd_core::state::StateStore::new(cwd.clone())
            .read()
            .unwrap()
            .current_phase,
        "BUILD_WAITING_AGENT"
    );

    let resumed = run(&CommandRequest {
        command: "auto".into(),
        cwd,
        args: Some(json!({ "resume": true })),
    })
    .unwrap();
    assert_eq!(resumed.state, "BUILD_WAITING_AGENT");
    assert!(resumed.action_required.is_some());
}

#[test]
fn auto_events_persist_in_runtime_json() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    let state = sdd_core::state::StateStore::new(cwd).read().unwrap();
    let run_id = state.active_loop.unwrap()["runId"]
        .as_str()
        .unwrap()
        .to_string();
    let runtime: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".sdd/runtime.json")).unwrap(),
    )
    .unwrap();
    assert!(runtime["loop"]["events"][run_id].as_array().unwrap().len() >= 2);
}

#[test]
fn auto_after_archive_reports_completed() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    // 走 new → design → plan
    let result = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    assert_eq!(result.state, "BUILD_WAITING_AGENT");
    // 模拟完成全部任务后再次 auto：推进 verify/review/archive
    let tasks = crate_helpers::complete_all_tasks(dir.path(), &cwd);
    assert!(tasks > 0, "应完成至少一个任务");
    let result2 = run(&CommandRequest {
        command: "auto".into(),
        cwd,
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    assert_eq!(result2.state, "ARCHIVED");
}

#[test]
fn auto_resume_retries_new_started_and_accepts_answers() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    let first = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({
            "requirement": "授权用户通过 POST /orders/{id}/cancel 请求取消待处理订单，入参 order_id，返回 status 和 error_code，返回取消成功"
        })),
    })
    .unwrap();
    assert_eq!(first.state, "CLARIFYING");

    sdd_core::state::StateStore::new(cwd.clone())
        .update(|state| {
            state.current_phase = "NEW_STARTED".into();
            state.in_progress_phase = Some("NEW_STARTED".into());
        })
        .unwrap();

    let paused = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "resume": true })),
    })
    .unwrap();
    assert_eq!(paused.state, "CLARIFYING");

    let resumed = run(&CommandRequest {
        command: "auto".into(),
        cwd,
        args: Some(json!({
            "resume": true,
            "answers": {
                "Q-ACTOR": "授权用户",
                "Q-AUTHORIZATION": "授权用户有权限",
                "Q-ACTION": "取消待处理订单",
                "Q-INTERFACE": "POST /orders/{id}/cancel，入参 order_id，返回 status 和 error_code",
                "Q-PRECONDITION": "订单处于待处理状态",
                "Q-RESULT": "返回取消成功",
                "Q-FAILURE": "取消待处理订单返回未授权错误",
                "Q-TEST": "需要自动化测试覆盖成功与未授权"
            }
        })),
    })
    .unwrap();
    assert_eq!(resumed.state, "BUILD_WAITING_AGENT");
}

#[test]
fn auto_resume_from_clarifying_accepts_answers() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    let first = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({
            "requirement": "授权用户通过 POST /orders/{id}/cancel 请求取消待处理订单，入参 order_id，返回 status 和 error_code，返回取消成功"
        })),
    })
    .unwrap();
    assert_eq!(first.state, "CLARIFYING");

    let resumed = run(&CommandRequest {
        command: "auto".into(),
        cwd,
        args: Some(json!({
            "resume": true,
            "answers": {
                "Q-ACTOR": "授权用户",
                "Q-AUTHORIZATION": "授权用户有权限",
                "Q-ACTION": "取消待处理订单",
                "Q-INTERFACE": "POST /orders/{id}/cancel，入参 order_id，返回 status 和 error_code",
                "Q-PRECONDITION": "订单处于待处理状态",
                "Q-RESULT": "返回取消成功",
                "Q-FAILURE": "取消待处理订单返回未授权错误",
                "Q-TEST": "需要自动化测试覆盖成功与未授权"
            }
        })),
    })
    .unwrap();
    assert_eq!(resumed.state, "BUILD_WAITING_AGENT");
}

#[test]
fn auto_step_failure_persists_paused_and_resume_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    let result = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    assert_eq!(result.state, "BUILD_WAITING_AGENT");

    // 模拟后续设计步骤失败：删掉 spec.md 并把阶段回退到 SPEC_READY，
    // 让 design 步骤的制品校验失败。
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let change_id = state.current_change_id.unwrap();
    std::fs::remove_file(
        dir.path()
            .join(".sdd/changes")
            .join(&change_id)
            .join("spec.md"),
    )
    .unwrap();
    sdd_core::state::StateStore::new(cwd.clone())
        .update(|state| {
            state.current_phase = "SPEC_READY".into();
            state.in_progress_phase = None;
            state.pending_agent_task = None;
            state.tasks.clear();
        })
        .unwrap();

    let failed = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert!(!failed.ok);
    assert_eq!(failed.state, "PAUSED");
    assert!(failed.error.is_some());

    // 恢复语义落地：持久化 current_phase=PAUSED、failed_command/failed_reason 存在
    let persisted = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    assert_eq!(persisted.current_phase, "PAUSED");
    assert_eq!(persisted.in_progress_phase, None);
    assert_eq!(persisted.failed_command.as_deref(), Some("sdd design"));
    assert!(persisted.failed_reason.is_some());
    assert_eq!(
        persisted.suggested_command.as_deref(),
        Some("sdd auto --resume")
    );
    let loop_status = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "loopStatus": true })),
    })
    .unwrap();
    assert_eq!(loop_status.data.unwrap()["activeLoop"]["status"], "FAILED");
    // sdd status 在 PAUSED 下返回 suggested_command 作为 next 建议
    let status = run(&CommandRequest {
        command: "status".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert_eq!(status.next.as_deref(), Some("sdd auto --resume"));

    // auto --resume 从 PAUSED 恢复：new 步骤重建 spec，链路推进到 Agent 边界
    let resumed = run(&CommandRequest {
        command: "auto".into(),
        cwd,
        args: Some(json!({ "resume": true })),
    })
    .unwrap();
    assert_eq!(resumed.state, "BUILD_WAITING_AGENT");
    assert!(resumed.action_required.is_some());
}

#[test]
fn auto_after_archive_opens_new_change_with_requirement() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    let result = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    assert_eq!(result.state, "BUILD_WAITING_AGENT");
    let tasks = crate_helpers::complete_all_tasks(dir.path(), &cwd);
    assert!(tasks > 0, "应完成至少一个任务");
    let archived = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    assert_eq!(archived.state, "ARCHIVED");
    let old_change_id = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap()
        .current_change_id
        .unwrap();
    let old_loop_run_id = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap()
        .active_loop
        .unwrap()["runId"]
        .as_str()
        .unwrap()
        .to_string();

    // 无需求且 ARCHIVED（当前变更仍为已归档变更）：保持 archive 幂等分支，返回已完成
    let idempotent = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert_eq!(idempotent.state, "ARCHIVED");

    // 归档后带新需求：ARCHIVED 加入 new 步骤，开启新变更（同名需求生成 -2 后缀 id）
    let opened = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    assert_eq!(opened.state, "BUILD_WAITING_AGENT");
    assert!(opened.action_required.is_some());
    let state = sdd_core::state::StateStore::new(cwd).read().unwrap();
    let new_change_id = state.current_change_id.unwrap();
    let new_loop_run_id = state.active_loop.unwrap()["runId"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(old_change_id, new_change_id, "应开启新变更");
    assert_ne!(
        old_loop_run_id, new_loop_run_id,
        "新变更应使用新的 auto run"
    );
}

#[test]
fn auto_tail_requires_events() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    let err = run(&CommandRequest {
        command: "auto".into(),
        cwd,
        args: Some(json!({ "tail": 5 })),
    })
    .unwrap_err();
    assert_eq!(err.code, "E_INVALID_PHASE_COMMAND");
    assert!(err.message.contains("--tail 必须与 --events"));
}

#[test]
fn auto_loop_blocks_phase_planning_commands_while_waiting_agent() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    let result = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    assert_eq!(result.state, "BUILD_WAITING_AGENT");

    // 会切换变更/阶段规划的命令被 E_CONCURRENT_RUN 拦截，next 建议 sdd auto --events
    for command in ["new", "change", "design", "plan"] {
        let err = run(&CommandRequest {
            command: command.into(),
            cwd: cwd.clone(),
            args: Some(json!({ "requirement": "新需求" })),
        })
        .unwrap_err();
        assert_eq!(err.code, "E_CONCURRENT_RUN", "{command} 应被拦截");
        assert_eq!(err.next.as_deref(), Some("sdd auto --events"));
    }
    let err = run(&CommandRequest {
        command: "codebase".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "sub": "index" })),
    })
    .unwrap_err();
    assert_eq!(err.code, "E_CONCURRENT_RUN");
    let err = run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap_err();
    assert_eq!(err.code, "E_CONCURRENT_RUN");

    // 放行命令：status / build next / codebase status 不受影响
    let status = run(&CommandRequest {
        command: "status".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert_eq!(status.state, "BUILD_WAITING_AGENT");
    let build = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "sub": "next" })),
    })
    .unwrap();
    assert!(build.ok);
    let codebase = run(&CommandRequest {
        command: "codebase".into(),
        cwd,
        args: Some(json!({ "sub": "status" })),
    })
    .unwrap();
    assert!(codebase.ok);
}

mod crate_helpers {
    use sdd_core::contracts::CommandRequest;
    use sdd_core::run;
    use serde_json::json;

    pub fn complete_all_tasks(_dir: &std::path::Path, cwd: &str) -> usize {
        let mut completed = 0;
        for _ in 0..20 {
            let next = run(&CommandRequest {
                command: "build".into(),
                cwd: cwd.to_string(),
                args: Some(json!({ "sub": "next" })),
            });
            let Ok(next) = next else { break };
            let Some(action) = next.action_required else {
                break;
            };
            let is_red = action.task_id.ends_with("-RED");
            // 提交与任务声明完全一致的验证命令；RED 阶段必须带失败证据与 failed 验证结果。
            let verification: Vec<serde_json::Value> = action
                .verification
                .iter()
                .map(|v| json!({ "command": v.command, "args": v.args, "passed": !is_red }))
                .collect();
            let full_command = action
                .verification
                .first()
                .map(|v| {
                    std::iter::once(v.command.as_str())
                        .chain(v.args.iter().map(|s| s.as_str()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let evidence = if action.task_id.ends_with("-VERIFY") {
                json!([])
            } else {
                json!([{ "type": "command-run", "command": full_command,
                    "output": if is_red { "FAILED: expected" } else { "ok" },
                    "passed": !is_red, "expectedFailure": is_red }])
            };
            let result_json = json!({
                "taskId": action.task_id,
                "status": "completed",
                "evidence": evidence,
                "verification": verification,
                "filesChanged": []
            })
            .to_string();
            let result = run(&CommandRequest {
                command: "build".into(),
                cwd: cwd.to_string(),
                args: Some(json!({
                    "sub": "complete",
                    "task": action.task_id,
                    "resultJson": result_json,
                })),
            });
            if result.is_err() {
                break;
            }
            completed += 1;
        }
        completed
    }
}
