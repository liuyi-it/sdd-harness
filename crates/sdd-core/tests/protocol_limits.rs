//! 任务结果协议限额测试：evidence/message/filesChanged/verification 超限拒绝。

use sdd_core::protocol::validate_task_result;
use serde_json::json;

/// 合法的最小任务结果基座
fn base() -> serde_json::Value {
    json!({
        "taskId": "TASK-001-RED",
        "status": "completed",
        "evidence": [{ "type": "command-run", "command": "cargo test", "output": "ok" }],
        "filesChanged": []
    })
}

fn evidence_item() -> serde_json::Value {
    json!({ "type": "command-run", "command": "cargo test", "output": "ok" })
}

#[test]
fn schema_convergence_rejects_missing_required() {
    // 缺 taskId：schema 结构校验失败，统一映射为协议错误码
    let raw = json!({ "status": "completed" });
    let err = validate_task_result(&raw).unwrap_err();
    assert_eq!(err.code, "E_TDD_EVIDENCE_REQUIRED");
}

#[test]
fn evidence_count_limit_is_64() {
    let mut over = base();
    over["evidence"] = json!((0..65).map(|_| evidence_item()).collect::<Vec<_>>());
    assert_eq!(
        validate_task_result(&over).unwrap_err().code,
        "E_TDD_EVIDENCE_REQUIRED"
    );
    let mut at_limit = base();
    at_limit["evidence"] = json!((0..64).map(|_| evidence_item()).collect::<Vec<_>>());
    assert!(validate_task_result(&at_limit).is_ok());
}

#[test]
fn evidence_output_length_limit_is_8192() {
    let mut over = base();
    over["evidence"][0]["output"] = json!("x".repeat(8193));
    assert_eq!(
        validate_task_result(&over).unwrap_err().code,
        "E_TDD_EVIDENCE_REQUIRED"
    );
    let mut at_limit = base();
    at_limit["evidence"][0]["output"] = json!("x".repeat(8192));
    assert!(validate_task_result(&at_limit).is_ok());
}

#[test]
fn message_length_limit_is_2048() {
    let mut over = base();
    over["message"] = json!("x".repeat(2049));
    assert_eq!(
        validate_task_result(&over).unwrap_err().code,
        "E_TDD_EVIDENCE_REQUIRED"
    );
    let mut at_limit = base();
    at_limit["message"] = json!("x".repeat(2048));
    assert!(validate_task_result(&at_limit).is_ok());
}

#[test]
fn files_changed_count_and_length_limits() {
    // 数量上限 500
    let mut over_count = base();
    over_count["filesChanged"] = json!((0..501)
        .map(|i| format!("src/file{i}.rs"))
        .collect::<Vec<_>>());
    assert_eq!(
        validate_task_result(&over_count).unwrap_err().code,
        "E_TDD_EVIDENCE_REQUIRED"
    );
    // 单条长度上限 512
    let mut over_length = base();
    over_length["filesChanged"] = json!(["src/lib.rs", "a".repeat(513)]);
    assert_eq!(
        validate_task_result(&over_length).unwrap_err().code,
        "E_TDD_EVIDENCE_REQUIRED"
    );
    // 边界内通过
    let mut ok = base();
    ok["filesChanged"] = json!(["src/lib.rs", "a".repeat(512)]);
    assert!(validate_task_result(&ok).is_ok());
}

#[test]
fn verification_count_limit_is_32() {
    let mut over = base();
    over["verification"] = json!((0..33)
        .map(|_| json!({ "command": "cargo test", "passed": true }))
        .collect::<Vec<_>>());
    assert_eq!(
        validate_task_result(&over).unwrap_err().code,
        "E_TDD_EVIDENCE_REQUIRED"
    );
    let mut at_limit = base();
    at_limit["verification"] = json!((0..32)
        .map(|_| json!({ "command": "cargo test", "passed": true }))
        .collect::<Vec<_>>());
    assert!(validate_task_result(&at_limit).is_ok());
}

#[test]
fn verification_command_argument_and_output_limits_are_enforced() {
    let mut over_command = base();
    over_command["verification"] = json!([{ "command": "x".repeat(2049) }]);
    assert_eq!(
        validate_task_result(&over_command).unwrap_err().code,
        "E_TDD_EVIDENCE_REQUIRED"
    );

    let mut over_args = base();
    over_args["verification"] = json!([{
        "command": "cargo test",
        "args": (0..65).map(|_| "--all").collect::<Vec<_>>()
    }]);
    assert_eq!(
        validate_task_result(&over_args).unwrap_err().code,
        "E_TDD_EVIDENCE_REQUIRED"
    );

    let mut over_output = base();
    over_output["verification"] = json!([{
        "command": "cargo test",
        "output": "x".repeat(8193)
    }]);
    assert_eq!(
        validate_task_result(&over_output).unwrap_err().code,
        "E_TDD_EVIDENCE_REQUIRED"
    );
}
