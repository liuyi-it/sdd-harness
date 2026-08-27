use sdd_core::state::artifact_store::{record_artifacts, verify_artifacts, ArtifactRecord};
use serde_json::json;

#[test]
fn records_and_updates_artifact_registry() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy();
    std::fs::write(dir.path().join("spec.md"), "one").unwrap();
    record_artifacts(
        &cwd,
        [ArtifactRecord {
            key: "spec",
            artifact_type: "spec",
            content_path: "spec.md",
            inputs: json!({}),
        }],
    )
    .unwrap();
    std::fs::write(dir.path().join("spec.md"), "two").unwrap();
    assert!(verify_artifacts(&cwd, ["spec"]).is_err());
    record_artifacts(
        &cwd,
        [ArtifactRecord {
            key: "spec",
            artifact_type: "spec",
            content_path: "spec.md",
            inputs: json!({}),
        }],
    )
    .unwrap();
    let registry: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".sdd/runtime.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        registry["artifacts"]["artifacts"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        registry["artifacts"]["artifacts"]["spec"]["hash"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    verify_artifacts(&cwd, ["spec"]).unwrap();
}

#[test]
fn rejects_corrupted_registry_and_escaping_paths() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy();
    std::fs::create_dir(dir.path().join(".sdd")).unwrap();
    std::fs::write(dir.path().join(".sdd/runtime.json"), "{").unwrap();
    let error = record_artifacts(
        &cwd,
        [ArtifactRecord {
            key: "spec",
            artifact_type: "spec",
            content_path: "spec.md",
            inputs: json!({}),
        }],
    )
    .unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");
    let error = record_artifacts(
        &cwd,
        [ArtifactRecord {
            key: "spec",
            artifact_type: "spec",
            content_path: "../spec.md",
            inputs: json!({}),
        }],
    )
    .unwrap_err();
    assert_eq!(error.code, "E_PATH_OUTSIDE_REPO");
}
