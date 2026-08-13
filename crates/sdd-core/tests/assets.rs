//! OMP 与 OpenCode 资产嵌入与写入测试。

use sdd_core::assets::{write_adapter_files, write_adapter_files_for, ADAPTER_ASSETS};

#[test]
fn supported_adapter_assets_are_embedded() {
    assert!(ADAPTER_ASSETS
        .iter()
        .all(|asset| asset.key.starts_with("omp/") || asset.key.starts_with("opencode/")));
}

#[test]
fn all_public_sdd_commands_are_embedded() {
    let commands = [
        "sdd.md",
        "sdd.init.md",
        "sdd.change.md",
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
fn subagent_profiles_use_configured_model_roles() {
    let config = ADAPTER_ASSETS
        .iter()
        .find(|asset| asset.target == ".omp/config.yml")
        .unwrap()
        .content;
    assert!(config.contains("smol: openai-codex/gpt-5.6-luna:medium"));
    assert!(config.contains("task: openai-codex/gpt-5.6-luna:max"));
    assert!(config.contains("slow: openai-codex/gpt-5.6-terra:max"));
    for (target, role) in [
        (".omp/agents/sdd-worker-simple.md", "@smol"),
        (".omp/agents/sdd-worker.md", "@task"),
        (".omp/agents/sdd-worker-complex.md", "@slow"),
    ] {
        let content = ADAPTER_ASSETS
            .iter()
            .find(|asset| asset.target == target)
            .unwrap()
            .content;
        assert!(content.contains(&format!("model: \"{role}\"")));
    }
}

#[test]
fn init_writes_omp_native_resources() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let written = write_adapter_files(&cwd).unwrap();
    let omp_assets: Vec<_> = ADAPTER_ASSETS
        .iter()
        .filter(|asset| asset.key.starts_with("omp/"))
        .collect();
    assert_eq!(written.len(), omp_assets.len());
    for asset in omp_assets {
        assert!(dir.path().join(asset.target).exists(), "{}", asset.target);
    }
}

#[test]
fn init_writes_opencode_native_resources() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let written = write_adapter_files_for(&cwd, "opencode").unwrap();
    let opencode_assets: Vec<_> = ADAPTER_ASSETS
        .iter()
        .filter(|asset| asset.key.starts_with("opencode/"))
        .collect();
    assert_eq!(written.len(), opencode_assets.len());
    for asset in opencode_assets {
        assert!(dir.path().join(asset.target).exists(), "{}", asset.target);
    }
    assert!(dir.path().join(".opencode/commands/sdd-new.md").exists());
    assert!(dir.path().join(".opencode/agents/sdd-worker.md").exists());
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
fn init_command_defaults_to_omp() {
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
fn init_command_supports_opencode() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let result = sdd_core::run(&sdd_core::contracts::CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: Some(serde_json::json!({ "hostAdapter": "opencode" })),
    })
    .unwrap();
    assert!(result.ok);
    assert!(dir
        .path()
        .join(".opencode/skills/sdd-harness/SKILL.md")
        .exists());
    let config = sdd_core::state::runtime_store::read_config(&cwd).unwrap();
    assert_eq!(config["plugins"]["opencode"]["enabled"], true);
}

#[test]
fn init_rejects_unsupported_host_adapter() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let result = sdd_core::run(&sdd_core::contracts::CommandRequest {
        command: "init".into(),
        cwd: dir.path().to_string_lossy().to_string(),
        args: Some(serde_json::json!({ "hostAdapter": "claude" })),
    });
    assert_eq!(result.unwrap_err().code, "E_INVALID_PHASE_COMMAND");
}

#[test]
fn init_rejects_non_string_host_adapter() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let result = sdd_core::run(&sdd_core::contracts::CommandRequest {
        command: "init".into(),
        cwd: dir.path().to_string_lossy().to_string(),
        args: Some(serde_json::json!({ "hostAdapter": ["opencode"] })),
    });
    assert_eq!(result.unwrap_err().code, "E_INVALID_PHASE_COMMAND");
}

#[test]
fn init_rejects_legacy_agent_argument() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let result = sdd_core::run(&sdd_core::contracts::CommandRequest {
        command: "init".into(),
        cwd: dir.path().to_string_lossy().to_string(),
        args: Some(serde_json::json!({ "agent": "opencode" })),
    });
    assert_eq!(result.unwrap_err().code, "E_INVALID_PHASE_COMMAND");
}
