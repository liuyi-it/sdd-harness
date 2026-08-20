//! 协议校验与策略编译测试。

use sdd_core::policies::digest::digest;
use sdd_core::policies::{compile_policy, resolve_policies};
use sdd_core::protocol::validate_task_result;
use serde_json::json;

#[test]
fn invalid_task_result_rejected() {
    // 缺 taskId
    let raw = json!({ "status": "completed" });
    let err = validate_task_result(&raw).unwrap_err();
    assert_eq!(err.code, "E_TDD_EVIDENCE_REQUIRED");
    // status 非法
    let raw = json!({ "taskId": "TASK-001-RED", "status": "done" });
    assert!(validate_task_result(&raw).is_err());
}

#[test]
fn valid_task_result_accepted() {
    let raw = json!({
        "taskId": "TASK-001-RED",
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
    assert_eq!(result.task_id, "TASK-001-RED");
    assert_eq!(result.evidence.len(), 1);
    assert_eq!(result.verification[0].args, vec!["--workspace"]);
    assert_eq!(result.verification[0].passed, Some(true));
    assert_eq!(result.files_changed, vec!["src/lib.rs"]);
}

#[test]
fn policy_compile_extracts_rules() {
    let md = "# Policy\n\n## 规则\n- 先写测试（RED）\n- 禁止修改未声明文件";
    let rules = compile_policy(md);
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].text, "先写测试（RED）");
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
fn resolve_policies_missing_dir_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let bundles = resolve_policies(&dir.path().join("nonexistent").to_string_lossy()).unwrap();
    assert!(bundles.is_empty());
}

#[test]
fn resolve_policies_reads_markdown() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("ponytail.md"),
        "# Policy\n\n## 规则\n- 最小正确实现",
    )
    .unwrap();
    let bundles = resolve_policies(&dir.path().to_string_lossy()).unwrap();
    assert_eq!(bundles.len(), 1);
    assert_eq!(bundles[0].name, "ponytail");
    assert_eq!(bundles[0].rules.len(), 1);
}
