//! 对外契约测试：JSON 序列化键名（camelCase）与退出码映射。

use sdd_core::contracts::{
    error_exit_codes, AgentActionRequired, CodebaseProviderInfo, CommandResult, VerificationCommand,
};

#[test]
fn command_result_serializes_camel_case() {
    let result = CommandResult {
        ok: true,
        state: "PLAN_READY".to_string(),
        exit_code: 0,
        change_id: Some("change-1".to_string()),
        next: Some("sdd build next".to_string()),
        data: None,
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    };
    let json = serde_json::to_value(&result).unwrap();
    // 键名必须是 camelCase（对齐 Node 版契约）
    assert!(json.get("exitCode").is_some(), "缺少 exitCode: {json}");
    assert!(json.get("changeId").is_some(), "缺少 changeId: {json}");
    assert!(json.get("exit_code").is_none(), "不允许 snake_case: {json}");
    assert!(json.get("change_id").is_none(), "不允许 snake_case: {json}");
}

#[test]
fn new_started_points_to_auto_resume() {
    assert_eq!(
        sdd_core::commands::status::next_command("NEW_STARTED").as_deref(),
        Some("sdd auto --resume")
    );
}

#[test]
fn new_started_without_ids_reports_corruption_and_status() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    sdd_core::run(&sdd_core::contracts::CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    sdd_core::state::StateStore::new(cwd.clone())
        .update(|state| {
            state.current_phase = "NEW_STARTED".into();
            state.current_change_id = None;
            state.current_run_id = None;
        })
        .unwrap();

    let error = sdd_core::run(&sdd_core::contracts::CommandRequest {
        command: "verify".into(),
        cwd,
        args: None,
    })
    .unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");
    assert_eq!(error.next.as_deref(), Some("sdd status"));
}

#[test]
fn action_required_serializes_camel_case() {
    let action = AgentActionRequired {
        action_type: "AGENT_TASK_EXECUTION".to_string(),
        task_id: "TASK-001-RED".to_string(),
        change_id: "change-1".to_string(),
        context_pack: "### Context Pack\n\n".to_string(),
        allowed_files: vec!["src/**".to_string()],
        expected_new_files: vec![],
        forbidden_files: vec![".env".to_string()],
        verification: vec![VerificationCommand {
            command: "cargo test".to_string(),
            args: vec![],
        }],
        result_transport: "inline-json".to_string(),
        codebase: CodebaseProviderInfo {
            provider: "codegraph".to_string(),
            degraded: false,
        },
        policy_bundle: None,
    };
    let json = serde_json::to_value(&action).unwrap();
    assert_eq!(
        json.get("type").and_then(|v| v.as_str()),
        Some("AGENT_TASK_EXECUTION")
    );
    assert!(json.get("contextPack").is_some());
    assert!(json.get("resultTransport").is_some());
    assert!(json.get("resultFile").is_none());
    let codebase = json.get("codebase").unwrap();
    assert!(codebase.get("provider").is_some());
    // provider 契约：codegraph | fallback-file-scan
    assert!(codebase
        .get("provider")
        .and_then(|v| v.as_str())
        .map(|p| ["codegraph", "fallback-file-scan"].contains(&p))
        .unwrap_or(false));
}

#[test]
fn workflow_state_serializes_camel_case() {
    let state = sdd_core::state::WorkflowState::not_initialized();
    let json = serde_json::to_value(&state).unwrap();
    assert!(
        json.get("currentPhase").is_some(),
        "缺少 currentPhase: {json}"
    );
    assert!(json.get("codebaseProvider").is_some());
    assert!(json.get("suggestedCommand").is_some());
    // schema 校验通过（camelCase 与 state.schema.json 一致）
    assert!(sdd_core::schema::validate_json("state", &json).is_ok());
}

#[test]
fn task_definition_serializes_camel_case() {
    let task = sdd_core::engines::superpowers::protocol::TaskDefinition {
        id: "TASK-001-RED".to_string(),
        title: "先写失败测试：x".to_string(),
        phase: "RED".to_string(),
        status: "PENDING".to_string(),
        requirements: vec!["REQ-001".to_string()],
        scenarios: vec![],
        depends_on: vec![],
        allowed_files: vec![],
        expected_new_files: vec![],
        forbidden_files: vec![],
        verification: vec![],
        done_criteria: vec![],
        slice_type: None,
        user_visible_outcome: None,
        acceptance_criteria: None,
        test_seam: None,
        policy_refs: None,
    };
    let json = serde_json::to_value(&task).unwrap();
    assert!(json.get("dependsOn").is_some(), "缺少 dependsOn: {json}");
    assert!(json.get("allowedFiles").is_some());
    assert!(json.get("doneCriteria").is_some());
}

#[test]
fn report_serializes_camel_case() {
    let report = sdd_core::quality::Report::new("verify", Some("change-1".to_string()));
    let json = serde_json::to_value(&report).unwrap();
    assert!(json.get("changeId").is_some());
    assert!(sdd_core::schema::validate_json("report", &json).is_ok());
}

#[test]
fn error_exit_codes_match_contract() {
    // 抽查关键错误码（与 Node 版 ERROR_EXIT_CODES 一致）
    assert_eq!(error_exit_codes("E_NOT_INITIALIZED"), 3);
    assert_eq!(error_exit_codes("E_SECURITY_BLOCKED"), 10);
    assert_eq!(error_exit_codes("E_VERIFY_FAILED"), 7);
    assert_eq!(error_exit_codes("E_LOCK_TIMEOUT"), 9);
    assert_eq!(error_exit_codes("E_TIMEOUT"), 124);
    assert_eq!(error_exit_codes("E_STATE_CORRUPTED"), 1);
    assert_eq!(error_exit_codes("E_REVIEW_BACKEND_UNAVAILABLE"), 5);
    assert_eq!(error_exit_codes("E_REVIEW_BACKEND_TIMEOUT"), 124);
    assert_eq!(error_exit_codes("E_REVIEW_BACKEND_FAILED"), 8);
    assert_eq!(error_exit_codes("E_REVIEW_BACKEND_INVALID_OUTPUT"), 8);
    // 未知错误码兜底 1
    assert_eq!(error_exit_codes("E_UNKNOWN"), 1);
}

#[test]
fn status_json_matches_contract_shape() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    sdd_core::run(&sdd_core::contracts::CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    let result = sdd_core::run(&sdd_core::contracts::CommandRequest {
        command: "status".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    let json = serde_json::to_value(&result).unwrap();
    assert!(json.get("exitCode").is_some());
    assert_eq!(
        json.get("state").and_then(|v| v.as_str()),
        Some("INDEX_READY")
    );
    assert_eq!(json.get("next").and_then(|v| v.as_str()), Some("sdd new"));
}
