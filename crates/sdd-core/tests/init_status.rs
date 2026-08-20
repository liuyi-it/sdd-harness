//! init 与 status 命令测试。

use sdd_core::commands::init::run_init;
use sdd_core::commands::status::run_status;
use sdd_core::contracts::CommandRequest;
use sdd_core::run;

fn req(dir: &std::path::Path, command: &str) -> CommandRequest {
    CommandRequest {
        command: command.into(),
        cwd: dir.to_string_lossy().to_string(),
        args: None,
    }
}

#[test]
fn init_creates_sdd_and_index_ready() {
    let dir = tempfile::tempdir().unwrap();
    // 放一个源文件使项目非空（避免空项目 warning 干扰断言）
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let result = run(&req(dir.path(), "init")).unwrap();
    assert!(result.ok);
    assert_eq!(result.state, "INDEX_READY");
    assert!(dir.path().join(".sdd/runtime.json").exists());
    let runtime: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".sdd/runtime.json")).unwrap(),
    )
    .unwrap();
    let config = &runtime["config"];
    assert_eq!(config["hostAdapter"], "codex");
    assert!(config.get("plugins").is_none());
    assert_eq!(runtime["config"]["quality"]["ocr"]["mode"], "auto");
    assert_eq!(runtime["config"]["quality"]["ocr"]["command"], "ocr");
    assert!(runtime["index"]["summary"].is_string());
    assert!(runtime["index"]["diagnostics"].is_array());
    assert!(!dir.path().join(".sdd/index").exists());
    assert!(dir
        .path()
        .join(".agents/skills/sdd-harness/SKILL.md")
        .exists());
    assert!(dir.path().join(".codex/agents/sdd-explorer.toml").exists());
    assert!(dir.path().join(".codex/agents/sdd-worker.toml").exists());
    assert!(dir.path().join(".codex/agents/sdd-reviewer.toml").exists());
    for obsolete in [
        "codebase-summary.md",
        "package-structure.md",
        "architecture.md",
    ] {
        assert!(!dir.path().join(".sdd/index").join(obsolete).exists());
    }
}

#[test]
fn status_after_init_reports_index_ready() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    run(&req(dir.path(), "init")).unwrap();
    let result = run(&req(dir.path(), "status")).unwrap();
    assert!(result.ok);
    assert_eq!(result.state, "INDEX_READY");
    assert_eq!(result.next.as_deref(), Some("sdd new"));
}

#[test]
fn status_before_init_is_not_initialized() {
    let dir = tempfile::tempdir().unwrap();
    let result = run(&req(dir.path(), "status")).unwrap();
    assert_eq!(result.state, "NOT_INITIALIZED");
    assert_eq!(result.next.as_deref(), Some("sdd init"));
}

#[test]
fn init_twice_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    run(&req(dir.path(), "init")).unwrap();
    let result = run(&req(dir.path(), "init")).unwrap();
    assert!(result.ok);
    assert_eq!(result.state, "INDEX_READY");
}

#[test]
fn build_on_uninitialized_returns_not_initialized() {
    let dir = tempfile::tempdir().unwrap();
    let err = run(&req(dir.path(), "build")).unwrap_err();
    assert_eq!(err.code, "E_NOT_INITIALIZED");
    assert_eq!(err.exit_code, 3);
}

#[test]
fn codebase_status_returns_codegraph_provider() {
    let dir = tempfile::tempdir().unwrap();
    let result = sdd_core::commands::codebase::run_codebase(
        dir.path().to_string_lossy().as_ref(),
        Some(&serde_json::json!({ "sub": "status" })),
    )
    .unwrap();
    assert!(result.ok);
    let data = result.data.expect("status 应有数据");
    assert_eq!(
        data.get("providers")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(1)
    );
}

#[test]
fn codebase_invalid_subcommand_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let err = sdd_core::commands::codebase::run_codebase(
        dir.path().to_string_lossy().as_ref(),
        Some(&serde_json::json!({ "sub": "frobnicate" })),
    )
    .unwrap_err();
    assert_eq!(err.code, "E_INVALID_PHASE_COMMAND");
}

#[test]
fn codebase_invalid_intent_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let err = sdd_core::commands::codebase::run_codebase(
        dir.path().to_string_lossy().as_ref(),
        Some(&serde_json::json!({
            "sub": "query", "query": "hello", "intent": "not-an-intent"
        })),
    )
    .unwrap_err();
    assert_eq!(err.code, "E_INVALID_PHASE_COMMAND");
}

#[test]
fn codebase_query_returns_payload() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let result = sdd_core::commands::codebase::run_codebase(
        dir.path().to_string_lossy().as_ref(),
        Some(&serde_json::json!({ "sub": "query", "query": "hello", "intent": "impact" })),
    )
    .unwrap();
    assert!(result.ok);
    let data = result.data.expect("query 应有数据");
    assert!(data.get("provider").is_some());
    assert!(data.get("degraded").is_some());
}

#[test]
fn run_status_directly_before_init() {
    let dir = tempfile::tempdir().unwrap();
    let result = run_status(dir.path().to_string_lossy().as_ref(), None).unwrap();
    assert_eq!(result.state, "NOT_INITIALIZED");
}

#[test]
fn run_init_directly_uses_lock() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let result = run_init(&cwd, None).unwrap();
    assert!(result.ok);
}

#[test]
fn empty_project_structure_policy_is_persisted_without_warning() {
    let dir = tempfile::tempdir().unwrap();
    let result = run(&CommandRequest {
        command: "init".into(),
        cwd: dir.path().to_string_lossy().to_string(),
        args: Some(serde_json::json!({ "structurePolicy": "free-design" })),
    })
    .unwrap();
    assert!(!result
        .warnings
        .unwrap_or_default()
        .iter()
        .any(|warning| warning["code"] == "W_EMPTY_PROJECT"));
    let runtime: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".sdd/runtime.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        runtime["config"]["workflow"]["structurePolicy"],
        "free-design"
    );
}

#[test]
fn init_rejects_structurally_invalid_existing_config() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    run(&req(dir.path(), "init")).unwrap();
    std::fs::write(dir.path().join(".sdd/runtime.json"), "{}").unwrap();
    std::fs::write(dir.path().join(".sdd/runtime.json.bak"), "{}").unwrap();
    let error = run(&req(dir.path(), "init")).unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");
}
