//! 任务结果协议与摘要测试。

use sdd_core::policies::digest::digest;
use sdd_core::protocol::validate_task_result;
use serde_json::json;

#[test]
fn invalid_task_result_rejected() {
    // 缺 taskId
    let raw = json!({ "status": "completed" });
    let err = validate_task_result(&raw).unwrap_err();
    assert_eq!(err.code, "E_TDD_EVIDENCE_REQUIRED");
    // status 非法
    let raw = json!({ "taskId": "TASK-001", "status": "done" });
    assert!(validate_task_result(&raw).is_err());
}

#[test]
fn valid_task_result_accepted() {
    let raw = json!({
        "taskId": "TASK-001",
        "status": "completed",
        "evidence": [
            { "type": "command-run", "command": "cargo test", "output": "FAIL",
              "passed": false, "expectedFailure": true }
        ],
        "verification": [
            { "command": "cargo test", "args": ["--workspace"], "passed": true, "output": "ok" }
        ],
        "filesChanged": ["src/lib.rs"]
    });
    let result = validate_task_result(&raw).unwrap();
    assert_eq!(result.task_id, "TASK-001");
    assert_eq!(result.evidence.len(), 1);
    assert_eq!(result.verification[0].args, vec!["--workspace"]);
    assert!(result.verification[0].passed);
    assert_eq!(result.files_changed, vec!["src/lib.rs"]);
}

#[test]
fn policy_digest_is_stable() {
    let a = digest("same content");
    let b = digest("same content");
    let c = digest("different");
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(a.len(), 64);
}
