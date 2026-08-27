//! schema 校验器测试。

use sdd_core::schema::{validate_json, SCHEMAS};
use serde_json::json;

fn valid_state() -> serde_json::Value {
    json!({
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
        "lastCommand": null,
        "previousPhase": null,
        "inProgressPhase": null,
        "failedCommand": null,
        "failedReason": null,
        "suggestedCommand": "sdd new",
        "tasks": {}
    })
}

fn valid_task(id: &str, phase: &str) -> serde_json::Value {
    json!({
        "id": id,
        "title": "实现行为",
        "phase": phase,
        "requirements": ["REQ-001"],
        "scenarios": ["SCN-001"],
        "dependsOn": [],
        "allowedFiles": ["src/lib.rs"],
        "expectedNewFiles": [],
        "forbiddenFiles": [".sdd/**"],
        "verification": ["cargo test"],
        "doneCriteria": ["测试通过"],
        "sliceType": "VERTICAL",
        "userVisibleOutcome": "用户行为通过验证",
        "acceptanceCriteria": ["场景通过"],
        "testSeam": "tests/lib.rs"
    })
}

#[test]
fn valid_state_passes() {
    let doc = valid_state();
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
fn all_seven_schemas_registered() {
    assert_eq!(SCHEMAS.len(), 7);
}

#[test]
fn config_accepts_only_current_complete_shape() {
    let current = json!({
        "schemaVersion": 3,
        "hostAdapter": "codex",
        "workflow": { "gitIsolation": false },
        "quality": { "ocr": { "mode": "auto", "command": "ocr" } },
        "contextPack": { "maxSizeKb": 30 },
        "audit": { "maxSizeMb": 5, "maxFiles": 200 }
    });
    assert!(validate_json("config", &current).is_ok());

    let mut obsolete = current;
    obsolete["plugins"] = json!({ "opencode": { "enabled": true } });
    assert!(validate_json("config", &obsolete).is_err());
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
    let doc = valid_task("TASK-001-RED", "RED");
    assert!(validate_json("task", &doc).is_ok());
}

#[test]
fn task_requires_nonempty_unique_execution_boundaries() {
    for field in [
        "allowedFiles",
        "forbiddenFiles",
        "verification",
        "requirements",
        "scenarios",
        "doneCriteria",
        "acceptanceCriteria",
    ] {
        let mut task = valid_task("TASK-001-RED", "RED");
        task[field] = json!([]);
        assert!(
            validate_json("task", &task).is_err(),
            "{field} 应拒绝空数组"
        );
    }
    let mut task = valid_task("TASK-001-RED", "RED");
    task["allowedFiles"] = json!(["src/lib.rs", "src/lib.rs"]);
    assert!(validate_json("task", &task).is_err());
}

#[test]
fn removed_task_and_evidence_fields_are_rejected() {
    let mut task = valid_task("TASK-001-RED", "RED");
    task["acceptance"] = json!(["旧字段"]);
    assert!(validate_json("task", &task).is_err());

    for evidence in [
        json!({ "type": "note", "command": "cargo test", "output": "ok" }),
        json!({ "type": "command-run", "command": "cargo test", "output": "ok", "file": "src/lib.rs" }),
        json!({ "type": "command-run", "command": "cargo test", "output": "ok", "args": [] }),
    ] {
        let result = json!({
            "taskId": "TASK-001-RED",
            "status": "completed",
            "evidence": [evidence],
            "verification": [{ "command": "cargo test", "passed": true }],
            "filesChanged": []
        });
        assert!(validate_json("task-result", &result).is_err());
    }
}

#[test]
fn invalid_task_phase_rejected() {
    let doc = valid_task("TASK-001-RED", "BLUE");
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
fn task_result_id_pattern_is_enforced() {
    let doc = json!({
        "taskId": "TASK-XYZ-RED",
        "status": "completed",
        "evidence": [],
        "verification": [{ "command": "cargo test", "passed": true }],
        "filesChanged": []
    });
    assert!(validate_json("task-result", &doc).is_err());
}

#[test]
fn valid_report_passes() {
    let doc = json!({
        "kind": "verify",
        "summary": "全部通过",
        "passed": true,
        "changeId": "demo",
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
    let doc = json!({
        "type": "spec",
        "hash": "a".repeat(64),
        "contentPath": "spec.md",
        "status": "READY",
        "inputs": {}
    });
    assert!(validate_json("artifact", &doc).is_ok());
}

#[test]
fn task_id_pattern_enforced() {
    // planner 生成格式 TASK-001-RED 通过
    let good = valid_task("TASK-001-RED", "RED");
    assert!(validate_json("task", &good).is_ok());
    let refactor = valid_task("TASK-002-REFACTOR", "REFACTOR");
    assert!(validate_json("task", &refactor).is_ok());
    // 非数字序号或未知阶段均不匹配 pattern
    let bad_id = valid_task("TASK-XYZ-RED", "RED");
    assert!(validate_json("task", &bad_id).is_err());
    let bad_phase = valid_task("TASK-001-BLUE", "BLUE");
    assert!(validate_json("task", &bad_phase).is_err());
}

#[test]
fn task_ref_phase_resolves() {
    // task.schema.json 的 phase 经 $ref 指向 $defs，解析后仍按枚举校验
    let good = valid_task("TASK-001-GREEN", "GREEN");
    assert!(validate_json("task", &good).is_ok());
    let bad = valid_task("TASK-001-GREEN", "NOT_A_PHASE");
    assert!(validate_json("task", &bad).is_err());
}

#[test]
fn minimum_keyword_enforced() {
    // report.schema.json 的 startLine/endLine 从 1 开始。
    let ok = json!({
        "kind": "verify",
        "summary": "ok",
        "passed": true,
        "changeId": "demo",
        "issues": [{
            "code": "X",
            "severity": "low",
            "message": "m",
            "startLine": 1,
            "endLine": 1
        }]
    });
    assert!(validate_json("report", &ok).is_ok());
    let bad = json!({
        "kind": "verify",
        "summary": "ok",
        "passed": true,
        "changeId": "demo",
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
    let mut doc = valid_state();
    doc["currentPhase"] = json!("BUILD_WAITING_AGENT");
    doc["currentChangeId"] = json!("change-1");
    doc["currentRunId"] = json!("run-1");
    doc["activeLoop"] = json!({
        "loopId": "loop-1",
        "runId": "run-1",
        "status": "WAITING_AGENT",
        "waiting": { "reason": "AGENT_TASK_EXECUTION", "since": "now" }
    });
    doc["pendingAgentTask"] = json!({
        "taskId": "TASK-001-RED",
        "since": "now",
        "gitBaseline": { "available": false }
    });
    doc["workspace"] = json!({
        "branchName": null,
        "worktreePath": null,
        "baselineCommit": "0000000000000000000000000000000000000000",
        "baselineChangedFiles": [],
        "baselineFileHashes": {},
        "baselineCargoManifest": null
    });
    assert!(validate_json("state", &doc).is_ok());
}

#[test]
fn state_rejects_loose_pending_workspace_and_task_keys() {
    let mut doc = valid_state();
    doc["workspace"] = json!({ "baselineCommit": "abc" });
    assert!(validate_json("state", &doc).is_err());

    let mut doc = valid_state();
    doc["pendingAgentTask"] = json!({ "taskId": "TASK-001-RED" });
    assert!(validate_json("state", &doc).is_err());

    let mut doc = valid_state();
    doc["tasks"] = json!({ "task-1": "PENDING" });
    assert!(validate_json("state", &doc).is_err());
}
