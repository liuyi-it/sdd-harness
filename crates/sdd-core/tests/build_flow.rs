//! build 命令流程测试。

use sdd_core::commands::new::run_new;
use sdd_core::contracts::CommandRequest;
use sdd_core::engines::spec::spec_engine::SpecEngine;
use sdd_core::run;
use serde_json::json;
use std::process::Command;

const FULL_REQUIREMENT: &str = "授权用户通过 POST /orders/{id}/cancel 请求取消待处理订单，入参 order_id，返回 status 和 error_code，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";

fn prepare(dir: &std::path::Path) -> String {
    std::fs::write(dir.join("README.md"), "# demo").unwrap();
    let cwd = dir.to_string_lossy().to_string();
    run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    sdd_core::state::runtime_store::write_index(
        &cwd,
        json!([]),
        "src/order_service.rs\nsrc/order_service.test.rs\nCargo.toml\n".to_string(),
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
    assert_eq!(action.result_transport, "inline-json");
    assert!(action.policy_bundle.is_some());
    // 契约变更：provider 必须是 CodeGraph 或受限文件扫描
    assert!(
        ["codegraph", "fallback-file-scan"].contains(&action.codebase.provider.as_str()),
        "未知 provider: {}",
        action.codebase.provider
    );
}

#[test]
fn build_rejects_tampered_plan() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let change_id = state.current_change_id.unwrap();
    let plan = sdd_core::state::runtime_store::read_change_field(&cwd, &change_id, "plan")
        .unwrap()
        .unwrap();
    let mut tampered = plan;
    tampered["tasks"] = json!([]);
    sdd_core::state::runtime_store::write_change_field(&cwd, &change_id, "plan", tampered).unwrap();

    let err = run(&CommandRequest {
        command: "build".into(),
        cwd,
        args: Some(json!({ "sub": "next" })),
    })
    .unwrap_err();
    assert_eq!(err.code, "E_COMPONENT_INTEGRITY_FAILED");
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
    let result_json =
        json!({ "taskId": action.task_id, "status": "completed", "evidence": [] }).to_string();
    let err = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({
            "sub": "complete",
            "task": action.task_id,
            "resultJson": result_json,
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
            "resultJson": "{}",
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
    let result_json = json!({
        "taskId": action.task_id,
        "status": "completed",
        "evidence": [
            { "type": "command-run", "command": "cargo test", "output": "FAILED: expected",
              "passed": false, "expectedFailure": true }
        ],
        "verification": [],
        "filesChanged": []
    })
    .to_string();
    let result = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({
            "sub": "complete",
            "task": action.task_id,
            "resultJson": result_json,
        })),
    })
    .unwrap();
    assert!(result.ok);
    // RED 完成后应返回 PLAN_READY 等待下一个任务
    assert_eq!(result.state, "PLAN_READY");
    assert_eq!(result.next.as_deref(), Some("sdd build next"));
}

#[test]
fn build_complete_rejects_unexpected_failure_as_red_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    let next = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "sub": "next" })),
    })
    .unwrap();
    let action = next.action_required.unwrap();
    let result_json = json!({
        "taskId": action.task_id,
        "status": "completed",
        "evidence": [{
            "type": "command-run", "command": "cargo test", "output": "编译器崩溃",
            "passed": false, "expectedFailure": false
        }],
        "verification": [],
        "filesChanged": []
    })
    .to_string();
    let err = run(&CommandRequest {
        command: "build".into(),
        cwd,
        args: Some(json!({
            "sub": "complete", "task": action.task_id, "resultJson": result_json
        })),
    })
    .unwrap_err();
    assert_eq!(err.code, "E_TDD_EVIDENCE_REQUIRED");
}

#[test]
fn build_complete_rejects_result_path_override() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    let next = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "sub": "next" })),
    })
    .unwrap();
    let action = next.action_required.unwrap();
    let err = run(&CommandRequest {
        command: "build".into(),
        cwd,
        args: Some(json!({
            "sub": "complete", "task": action.task_id, "result": "../result.json"
        })),
    })
    .unwrap_err();
    assert_eq!(err.code, "E_INVALID_PHASE_COMMAND");
}
#[test]
fn build_complete_rejects_files_changed_that_disagree_with_git() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "test"],
        vec!["add", "."],
        vec!["commit", "-qm", "baseline"],
    ] {
        let output = Command::new("git")
            .args(&args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let next = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "sub": "next" })),
    })
    .unwrap();
    let action = next.action_required.unwrap();
    let changed = action
        .allowed_files
        .iter()
        .find(|path| !path.contains('*'))
        .expect("计划应包含可修改文件")
        .clone();
    let changed_path = dir.path().join(changed);
    std::fs::create_dir_all(changed_path.parent().unwrap()).unwrap();
    std::fs::write(changed_path, "task change").unwrap();
    let result_json = json!({
        "taskId": action.task_id,
        "status": "completed",
        "evidence": [{
            "type": "command-run", "command": "cargo test", "output": "expected failure",
            "passed": false, "expectedFailure": true
        }],
        "verification": [],
        "filesChanged": []
    })
    .to_string();
    let err = run(&CommandRequest {
        command: "build".into(),
        cwd,
        args: Some(json!({
            "sub": "complete", "task": action.task_id, "resultJson": result_json
        })),
    })
    .unwrap_err();
    assert_eq!(err.code, "E_UNDECLARED_FILE_CHANGE");
}

#[test]
fn build_next_accepts_existing_changes_from_completed_task() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "test"],
        vec!["add", "."],
        vec!["commit", "-qm", "baseline"],
    ] {
        let output = Command::new("git")
            .args(&args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let first = run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "sub": "next" })),
    })
    .unwrap()
    .action_required
    .unwrap();
    let changed = if first.expected_new_files.is_empty() {
        vec![first
            .allowed_files
            .iter()
            .find(|path| !path.contains('*'))
            .unwrap()
            .clone()]
    } else {
        first.expected_new_files.clone()
    };
    for path in &changed {
        let target = dir.path().join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, "red test").unwrap();
    }
    let result_json = json!({
        "taskId": first.task_id,
        "status": "completed",
        "evidence": [{
            "type": "command-run", "command": "cargo test", "output": "expected failure",
            "passed": false, "expectedFailure": true
        }],
        "verification": [],
        "filesChanged": changed
    })
    .to_string();
    run(&CommandRequest {
        command: "build".into(),
        cwd: cwd.clone(),
        args: Some(json!({
            "sub": "complete", "task": first.task_id, "resultJson": result_json
        })),
    })
    .unwrap();
    let second = run(&CommandRequest {
        command: "build".into(),
        cwd,
        args: Some(json!({ "sub": "next" })),
    })
    .unwrap();
    assert_eq!(second.state, "BUILD_WAITING_AGENT");
    assert!(second.action_required.unwrap().task_id.ends_with("-GREEN"));
}
