use sdd_core::contracts::CommandRequest;
use sdd_core::run;
use serde_json::Value;

#[test]
fn init_persists_all_machine_data_in_runtime_json() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();

    let result = run(&CommandRequest {
        command: "init".into(),
        cwd,
        args: None,
    })
    .unwrap();
    assert_eq!(result.state, "INDEX_READY");

    let runtime_path = dir.path().join(".sdd/runtime.json");
    assert!(runtime_path.exists());
    assert!(dir.path().join(".sdd/runtime.json.sha256").exists());
    assert!(!dir.path().join(".sdd/runtime.json.hmac").exists());
    let runtime: Value =
        serde_json::from_str(&std::fs::read_to_string(runtime_path).unwrap()).unwrap();
    assert!(sdd_core::schema::validate_json("runtime", &runtime).is_ok());
    for key in [
        "schemaVersion",
        "state",
        "config",
        "artifacts",
        "changes",
        "runs",
        "loop",
        "index",
    ] {
        assert!(runtime.get(key).is_some(), "runtime 缺少 {key}");
    }
    for path in [
        ".sdd/state.json",
        ".sdd/state.json.bak",
        ".sdd/config.json",
        ".sdd/artifacts.json",
        ".sdd/index/knowledge.json",
        ".sdd/index/summary.md",
    ] {
        assert!(!dir.path().join(path).exists(), "不应生成 {path}");
    }
}

#[test]
fn batched_field_writes_persist_related_runtime_data_together() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();

    sdd_core::state::runtime_store::write_run_fields(
        &cwd,
        "run-1",
        [
            ("input", serde_json::json!("实现批量写入")),
            ("answers", serde_json::json!({ "Q-GOAL": "验证" })),
        ],
    )
    .unwrap();
    sdd_core::state::runtime_store::write_change_fields(
        &cwd,
        "change-1",
        [
            ("spec", serde_json::json!({ "status": "READY" })),
            ("design", serde_json::json!("设计")),
        ],
    )
    .unwrap();

    let runtime = sdd_core::state::RuntimeStore::new(cwd).read().unwrap();
    assert_eq!(runtime.runs["run-1"]["input"], "实现批量写入");
    assert_eq!(runtime.runs["run-1"]["answers"]["Q-GOAL"], "验证");
    assert_eq!(runtime.changes["change-1"]["spec"]["status"], "READY");
    assert_eq!(runtime.changes["change-1"]["design"], "设计");
}

#[test]
fn change_runtime_fields_reject_unsafe_change_ids() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();

    let error = sdd_core::state::runtime_store::write_change_field(
        &cwd,
        "../outside",
        "spec",
        serde_json::json!({}),
    )
    .unwrap_err();

    assert_eq!(error.code, "E_SECURITY_BLOCKED");
}
