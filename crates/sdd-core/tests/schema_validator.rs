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
        "codebaseProvider": "unknown-provider"
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

#[test]
fn task_id_pattern_enforced() {
    // planner 生成格式 TASK-001-RED 通过
    let good = json!({
        "id": "TASK-001-RED",
        "title": "x",
        "phase": "RED",
        "status": "PENDING"
    });
    assert!(validate_json("task", &good).is_ok());
    let refactor = json!({
        "id": "TASK-002-REFACTOR",
        "title": "x",
        "phase": "REFACTOR",
        "status": "PENDING"
    });
    assert!(validate_json("task", &refactor).is_ok());
    // 非数字序号或未知阶段均不匹配 pattern
    let bad_id = json!({
        "id": "TASK-XYZ-RED",
        "title": "x",
        "phase": "RED",
        "status": "PENDING"
    });
    assert!(validate_json("task", &bad_id).is_err());
    let bad_phase = json!({
        "id": "TASK-001-BLUE",
        "title": "x",
        "phase": "BLUE",
        "status": "PENDING"
    });
    assert!(validate_json("task", &bad_phase).is_err());
}

#[test]
fn task_ref_phase_and_status_resolve() {
    // task.schema.json 的 phase/status 经 $ref 指向 $defs，解析后仍按枚举校验
    let good = json!({
        "id": "TASK-001-GREEN",
        "title": "x",
        "phase": "GREEN",
        "status": "DONE"
    });
    assert!(validate_json("task", &good).is_ok());
    let bad = json!({
        "id": "TASK-001-GREEN",
        "title": "x",
        "phase": "GREEN",
        "status": "NOT_A_STATUS"
    });
    assert!(validate_json("task", &bad).is_err());
}

#[test]
fn minimum_keyword_enforced() {
    // report.schema.json 的 startLine/endLine 带 minimum: 0
    let ok = json!({
        "kind": "verify",
        "summary": "ok",
        "passed": true,
        "issues": [{
            "code": "X",
            "severity": "low",
            "message": "m",
            "startLine": 0,
            "endLine": 1
        }]
    });
    assert!(validate_json("report", &ok).is_ok());
    let bad = json!({
        "kind": "verify",
        "summary": "ok",
        "passed": true,
        "issues": [{
            "code": "X",
            "severity": "low",
            "message": "m",
            "startLine": -1,
            "endLine": 0
        }]
    });
    assert!(validate_json("report", &bad).is_err());
}

#[test]
fn state_accepts_loop_agent_and_workspace_fields() {
    let doc = json!({
        "currentPhase": "BUILD_WAITING_AGENT",
        "indexStatus": "INDEX_READY",
        "activeLoop": { "runId": "run-1" },
        "pendingAgentTask": { "taskId": "TASK-001-RED" },
        "workspace": { "baselineCommit": "abc" }
    });
    assert!(validate_json("state", &doc).is_ok());
}
