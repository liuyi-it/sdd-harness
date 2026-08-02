use sdd_core::state::artifact_store::{record_artifact, verify_artifact};
use serde_json::json;

#[test]
fn records_and_updates_artifact_registry() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy();
    record_artifact(&cwd, "spec", "spec", "spec.md", "one", json!({})).unwrap();
    record_artifact(&cwd, "spec", "spec", "spec.md", "two", json!({})).unwrap();
    let registry: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".sdd/artifacts.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(registry["artifacts"].as_object().unwrap().len(), 1);
    assert_eq!(
        registry["artifacts"]["spec"]["hash"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert!(verify_artifact(&cwd, "spec").is_err());
    std::fs::write(dir.path().join("spec.md"), "two").unwrap();
    verify_artifact(&cwd, "spec").unwrap();
}

#[test]
fn rejects_corrupted_registry_and_escaping_paths() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy();
    std::fs::create_dir(dir.path().join(".sdd")).unwrap();
    std::fs::write(dir.path().join(".sdd/artifacts.json"), "{").unwrap();
    let error = record_artifact(&cwd, "spec", "spec", "spec.md", "one", json!({})).unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");
    let error = record_artifact(&cwd, "spec", "spec", "../spec.md", "one", json!({})).unwrap_err();
    assert_eq!(error.code, "E_PATH_OUTSIDE_REPO");
}
