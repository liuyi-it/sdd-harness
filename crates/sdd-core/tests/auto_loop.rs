//! auto 命令测试。

use sdd_core::contracts::CommandRequest;
use sdd_core::run;
use serde_json::json;

const FULL_REQUIREMENT: &str = "授权用户通过 API 请求取消待处理订单，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";

fn setup(dir: &std::path::Path) -> String {
    std::fs::write(dir.join("README.md"), "# demo").unwrap();
    let cwd = dir.to_string_lossy().to_string();
    run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    std::fs::create_dir_all(dir.join(".sdd/index")).unwrap();
    std::fs::write(
        dir.join(".sdd/index/summary.md"),
        "src/order_service.rs\nsrc/order_service.test.rs\nCargo.toml\n",
    )
    .unwrap();
    cwd
}

#[test]
fn auto_pauses_on_missing_requirement() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    let result = run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
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
fn auto_events_reject_corrupted_jsonl() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let cwd = setup(dir.path());
    run(&CommandRequest {
        command: "auto".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let run_id = state.active_loop.unwrap()["runId"]
        .as_str()
        .unwrap()
        .to_string();
    let path = dir
        .path()
        .join(".sdd/loop/events")
        .join(format!("{run_id}.jsonl"));
    writeln!(
        std::fs::OpenOptions::new().append(true).open(path).unwrap(),
        "{{broken"
    )
    .unwrap();

    let error = run(&CommandRequest {
        command: "auto".into(),
        cwd,
        args: Some(json!({ "events": true })),
    })
    .unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");
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
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    assert_eq!(result2.state, "ARCHIVED");
}

mod crate_helpers {
    use sdd_core::contracts::CommandRequest;
    use sdd_core::run;
    use serde_json::json;

    pub fn complete_all_tasks(dir: &std::path::Path, cwd: &str) -> usize {
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
            let result_path = dir.join(&action.result_file);
            if let Some(parent) = result_path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let is_verify = action.task_id.ends_with("-VERIFY");
            let is_red = action.task_id.ends_with("-RED");
            let evidence = if is_verify {
                json!([])
            } else {
                json!([{ "type": "command-run", "command": "cargo test",
                    "output": if is_red { "FAILED: expected" } else { "ok" },
                    "passed": !is_red, "expectedFailure": is_red }])
            };
            std::fs::write(
                &result_path,
                json!({
                    "taskId": action.task_id,
                    "status": "completed",
                    "evidence": evidence,
                    "verification": if is_verify {
                        json!([{ "command": "cargo test", "args": [], "passed": true }])
                    } else { json!([]) },
                    "filesChanged": []
                })
                .to_string(),
            )
            .unwrap();
            let result = run(&CommandRequest {
                command: "build".into(),
                cwd: cwd.to_string(),
                args: Some(json!({
                    "sub": "complete",
                    "task": action.task_id,
                    "result": action.result_file,
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
