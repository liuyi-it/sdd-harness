//! OMP 资产嵌入与写入测试。

use sdd_core::assets::{write_adapter_files, ADAPTER_ASSETS};

#[test]
fn only_omp_assets_are_embedded() {
    assert!(ADAPTER_ASSETS
        .iter()
        .all(|asset| asset.key.starts_with("omp/")));
}

#[test]
fn all_public_sdd_commands_are_embedded() {
    let commands = [
        "sdd.md",
        "sdd.init.md",
        "sdd.status.md",
        "sdd.plan.md",
        "sdd.verify.md",
        "sdd.review.md",
        "sdd.archive.md",
    ];
    for command in commands {
        assert!(ADAPTER_ASSETS
            .iter()
            .any(|asset| asset.key == format!("omp/commands/{command}")));
    }
}

#[test]
fn init_writes_omp_native_resources() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let written = write_adapter_files(&cwd).unwrap();
    assert_eq!(written.len(), ADAPTER_ASSETS.len());
    for asset in ADAPTER_ASSETS {
        assert!(dir.path().join(asset.target).exists(), "{}", asset.target);
    }
}

#[test]
fn write_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    write_adapter_files(&cwd).unwrap();
    let second = write_adapter_files(&cwd).unwrap();
    assert!(second.is_empty());
}

#[test]
fn stale_omp_resources_are_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    write_adapter_files(&cwd).unwrap();
    let skill = dir.path().join(".omp/skills/sdd-harness/SKILL.md");
    std::fs::write(&skill, "旧模板").unwrap();

    let written = write_adapter_files(&cwd).unwrap();

    assert!(written
        .iter()
        .any(|item| item.contains("写入：.omp/skills")));
    assert_eq!(
        std::fs::read_to_string(skill).unwrap(),
        ADAPTER_ASSETS
            .iter()
            .find(|asset| asset.target == ".omp/skills/sdd-harness/SKILL.md")
            .unwrap()
            .content
    );
}

#[test]
fn init_command_integrates_only_omp() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let result = sdd_core::run(&sdd_core::contracts::CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert!(result.ok);
    assert!(dir.path().join(".omp/skills/sdd-harness/SKILL.md").exists());
}

#[test]
fn init_rejects_agent_selection() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let result = sdd_core::run(&sdd_core::contracts::CommandRequest {
        command: "init".into(),
        cwd: dir.path().to_string_lossy().to_string(),
        args: Some(serde_json::json!({ "agent": "other" })),
    });
    assert_eq!(result.unwrap_err().code, "E_INVALID_PHASE_COMMAND");
}
