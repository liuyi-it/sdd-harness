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
