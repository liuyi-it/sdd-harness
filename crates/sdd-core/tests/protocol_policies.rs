//! 任务结果协议与摘要测试。

use sdd_core::policies::digest::digest;
use sdd_core::protocol::validate_task_result;
use sdd_core::schema::validate_json;
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
        "collaboration": {
            "topology": "inline",
            "lead": "developer",
            "contributions": [],
            "limitations": []
        },
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

#[test]
fn collaboration_topology_rules_are_enforced_after_schema_validation() {
    let pipeline_without_predecessor = json!({
        "topology": "pipeline",
        "lead": "architect",
        "contributions": [
            {
                "id": "product",
                "role": "product",
                "summary": "整理用户目标",
                "inputs": [],
                "feedback": []
            },
            {
                "id": "architect",
                "role": "architect",
                "summary": "检查代码边界",
                "inputs": [],
                "feedback": []
            }
        ],
        "limitations": []
    });
    let error = validate_json("collaboration", &pipeline_without_predecessor).unwrap_err();
    assert!(error.message.contains("pipeline"));

    let mob_with_one_role_and_no_feedback = json!({
        "topology": "mob",
        "lead": "architect",
        "contributions": [
            {
                "id": "developer-a",
                "role": "developer",
                "summary": "实现方案",
                "inputs": [],
                "feedback": []
            },
            {
                "id": "developer-b",
                "role": "developer",
                "summary": "复核方案",
                "inputs": [],
                "feedback": []
            }
        ],
        "limitations": []
    });
    let error = validate_json("collaboration", &mob_with_one_role_and_no_feedback).unwrap_err();
    assert!(error.message.contains("mob"));

    let empty_subagent = json!({
        "topology": "subagent",
        "lead": "developer",
        "contributions": [],
        "limitations": []
    });
    let error = validate_json("collaboration", &empty_subagent).unwrap_err();
    assert!(error.message.contains("贡献"));
}
