//! schema 校验器测试。

use sdd_core::schema::{validate_json, SCHEMAS};
use serde_json::json;

#[test]
fn valid_state_passes() {
    let doc = json!({
        "schemaVersion": 3,
        "version": 1,
        "updatedAt": "2026-08-02T00:00:00Z",
        "initialized": true,
        "currentChangeId": null,
        "currentRunId": null,
        "currentPhase": "INDEX_READY",
        "indexStatus": "INDEX_READY",
        "codebaseProvider": "codegraph",
        "degraded": false,
        "degradedReason": null,
        "tasks": {},
        "artifacts": {}
    });
    assert!(validate_json("state", &doc).is_ok());
}

#[test]
fn invalid_state_rejected() {
    let doc = json!({ "currentPhase": "NOT_A_PHASE" });
    assert!(validate_json("state", &doc).is_err());
}

#[test]
fn invalid_codebase_provider_rejected() {
    let doc = json!({
        "currentPhase": "INDEX_READY",
        "indexStatus": "INDEX_READY",
        "codebaseProvider": "legacy-provider"
    });
    assert!(validate_json("state", &doc).is_err());
}

#[test]
fn all_six_schemas_registered() {
    assert_eq!(SCHEMAS.len(), 6);
    assert_eq!(sdd_core::schema::schema_names().len(), 6);
}

#[test]
fn array_items_are_validated() {
    let doc = json!({
        "taskId": "TASK-001-RED",
        "status": "completed",
        "evidence": [{ "type": "invalid", "command": "cargo test", "output": "failed" }]
    });
    assert!(validate_json("task-result", &doc).is_err());
}

#[test]
fn valid_task_passes() {
    let doc = json!({
        "id": "TASK-001-RED",
        "title": "编写失败测试",
        "phase": "RED",
        "status": "PENDING"
    });
    assert!(validate_json("task", &doc).is_ok());
}

#[test]
fn invalid_task_phase_rejected() {
    let doc = json!({
        "id": "TASK-001-RED",
        "title": "x",
        "phase": "BLUE",
        "status": "PENDING"
    });
    assert!(validate_json("task", &doc).is_err());
}

#[test]
fn valid_task_result_passes() {
    let doc = json!({
        "taskId": "TASK-001-RED",
        "status": "completed",
        "evidence": [
            { "type": "command-run", "command": "cargo test", "output": "ok" }
        ],
        "verification": [
            { "command": "cargo test", "args": [], "passed": true }
        ],
        "filesChanged": ["src/lib.rs"]
    });
    assert!(validate_json("task-result", &doc).is_ok());
}

#[test]
fn valid_report_passes() {
    let doc = json!({
        "kind": "verify",
        "summary": "全部通过",
        "passed": true,
        "issues": []
    });
    assert!(validate_json("report", &doc).is_ok());
}

#[test]
fn report_schema_accepts_ocr_optional_fields() {
    let doc = json!({
        "kind": "review",
        "summary": "ok",
        "passed": true,
        "changeId": "demo",
        "issues": [{
            "code": "OCR_FINDING",
            "severity": "medium",
            "message": "建议处理错误",
            "file": "src/a.rs",
            "category": "bug",
            "startLine": 1,
            "endLine": 1,
            "suggestionCode": "return Err(err);",
            "origin": "ocr"
        }]
    });
    assert!(validate_json("report", &doc).is_ok());
}

#[test]
fn invalid_report_kind_rejected() {
    let doc = json!({ "kind": "nonsense", "summary": "x", "passed": true });
    assert!(validate_json("report", &doc).is_err());
}

#[test]
fn valid_artifact_passes() {
    let doc = json!({ "type": "spec", "hash": "abc123", "contentPath": "spec.md" });
    assert!(validate_json("artifact", &doc).is_ok());
}
