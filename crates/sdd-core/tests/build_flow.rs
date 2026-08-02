//! build 命令流程测试。

use sdd_core::commands::new::run_new;
use sdd_core::contracts::CommandRequest;
use sdd_core::engines::spec::spec_engine::SpecEngine;
use sdd_core::run;
use serde_json::json;

const FULL_REQUIREMENT: &str = "授权用户通过 API 请求取消待处理订单，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";

fn prepare(dir: &std::path::Path) -> String {
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
        dir.join(".sdd/index/codebase-summary.md"),
        "src/order_service.rs\nsrc/order_service.test.rs\nCargo.toml\n",
    )
    .unwrap();
    run_new(
        &cwd,
        Some(&json!({ "requirement": FULL_REQUIREMENT })),
        &SpecEngine::new(),
    )
    .unwrap();
    run(&CommandRequest {
        command: "design".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    let plan = run(&CommandRequest {
        command: "plan".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert!(plan.ok, "plan 应成功: {:?}", plan.error);
    cwd
}

#[test]
fn build_next_returns_action_required_with_known_provider() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    let result = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "sub": "next" })),
    })
    .unwrap();
    assert!(result.ok);
    assert_eq!(result.state, "BUILD_WAITING_AGENT");
    let action = result.action_required.expect("应有 actionRequired");
    assert_eq!(action.action_type, "AGENT_TASK_EXECUTION");
    assert!(action.task_id.starts_with("TASK-"));
    assert!(action.result_file.ends_with(".result.json"));
    // 契约变更：provider 必须是三个合法值之一
    assert!(
        ["gitnexus", "codegraph", "fallback-file-scan"]
            .contains(&action.codebase.provider.as_str()),
        "未知 provider: {}",
        action.codebase.provider
    );
}

#[test]
fn build_complete_with_invalid_result_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    let next = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "sub": "next" })),
    })
    .unwrap();
    let action = next.action_required.unwrap();
    // 伪造结果：缺少 evidence
    let result_path = dir.path().join(&action.result_file);
    std::fs::create_dir_all(result_path.parent().unwrap()).unwrap();
    std::fs::write(
        &result_path,
        json!({ "taskId": action.task_id, "status": "completed", "evidence": [] }).to_string(),
    )
    .unwrap();
    let err = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({
            "sub": "complete",
            "task": action.task_id,
            "result": action.result_file,
        })),
    })
    .unwrap_err();
    assert_eq!(err.code, "E_TDD_EVIDENCE_REQUIRED");
    assert_eq!(err.exit_code, 7);
}

#[test]
fn build_complete_wrong_task_id_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "sub": "next" })),
    })
    .unwrap();
    let err = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({
            "sub": "complete",
            "task": "TASK-999-RED",
            "result": ".sdd/runs/run-x/tasks/TASK-999-RED.result.json",
        })),
    })
    .unwrap_err();
    assert_eq!(err.code, "E_AGENT_TASK_FAILED");
}

#[test]
fn build_complete_with_valid_result_passes_red() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    let next = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "sub": "next" })),
    })
    .unwrap();
    let action = next.action_required.unwrap();
    assert!(action.task_id.ends_with("-RED"));
    let result_path = dir.path().join(&action.result_file);
    std::fs::create_dir_all(result_path.parent().unwrap()).unwrap();
    std::fs::write(
        &result_path,
        json!({
            "taskId": action.task_id,
            "status": "completed",
            "evidence": [
                { "type": "command-run", "command": "cargo test", "output": "FAILED: expected" }
            ],
            "verification": [],
            "filesChanged": []
        })
        .to_string(),
    )
    .unwrap();
    let result = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({
            "sub": "complete",
            "task": action.task_id,
            "result": action.result_file,
        })),
    })
    .unwrap();
    assert!(result.ok);
    // RED 完成后应返回 PLAN_READY 等待下一个任务
    assert_eq!(result.state, "PLAN_READY");
    assert_eq!(result.next.as_deref(), Some("sdd build next"));
}
