//! Agent 资产集成测试：通过公开 init 契约验证宿主接入，不暴露资产层内部实现。

use sdd_core::contracts::CommandRequest;

fn run_init(cwd: &str, host_adapter: Option<&str>) -> sdd_core::contracts::CommandResult {
    let args = host_adapter.map(|adapter| serde_json::json!({ "hostAdapter": adapter }));
    sdd_core::run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.to_string(),
        args,
    })
    .unwrap()
}

#[test]
fn init_defaults_to_codex_native_resources() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy();

    let result = run_init(&cwd, None);

    assert!(result.ok);
    assert!(dir
        .path()
        .join(".agents/skills/sdd-harness/SKILL.md")
        .exists());
    assert!(dir.path().join(".codex/agents/sdd-explorer.toml").exists());
    assert!(dir.path().join(".codex/agents/sdd-worker.toml").exists());
    assert!(dir
        .path()
        .join(".codex/agents/sdd-worker-complex.toml")
        .exists());
    assert!(dir.path().join(".codex/agents/sdd-reviewer.toml").exists());
    assert!(dir.path().join(".codex/agents/sdd-architect.toml").exists());
    assert!(!dir.path().join(".opencode").exists());

    let config = sdd_core::state::RuntimeStore::new(cwd.to_string())
        .read()
        .unwrap()
        .config;
    assert_eq!(config["hostAdapter"], "codex");
    assert!(config.get("plugins").is_none());
}

#[test]
fn codex_assets_define_focused_skill_and_subagent_roles() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy();
    run_init(&cwd, Some("codex"));

    let skill =
        std::fs::read_to_string(dir.path().join(".agents/skills/sdd-harness/SKILL.md")).unwrap();
    assert!(skill.contains("name: sdd-harness"));
    assert!(skill.contains("sdd-explorer"));
    assert!(skill.contains("sdd-worker"));
    assert!(skill.contains("sdd-worker-complex"));
    assert!(skill.contains("sdd-reviewer"));
    assert!(skill.contains("sdd-architect"));
    assert!(skill.contains("不要并行编辑共享文件"));
    assert!(skill.contains("不得无声降级"));

    for (file, name, model, effort, sandbox) in [
        (
            "sdd-explorer.toml",
            "sdd-explorer",
            "gpt-5.6-terra",
            "max",
            "read-only",
        ),
        (
            "sdd-worker.toml",
            "sdd-worker",
            "gpt-5.6-luna",
            "max",
            "workspace-write",
        ),
        (
            "sdd-worker-complex.toml",
            "sdd-worker-complex",
            "gpt-5.6-terra",
            "max",
            "workspace-write",
        ),
        (
            "sdd-reviewer.toml",
            "sdd-reviewer",
            "gpt-5.6-terra",
            "max",
            "read-only",
        ),
        (
            "sdd-architect.toml",
            "sdd-architect",
            "gpt-5.6-sol",
            "xhigh",
            "read-only",
        ),
    ] {
        let content = std::fs::read_to_string(dir.path().join(".codex/agents").join(file)).unwrap();
        assert!(content.contains(&format!("name = \"{name}\"")));
        assert!(content.contains(&format!("model = \"{model}\"")));
        assert!(content.contains(&format!("model_reasoning_effort = \"{effort}\"")));
        assert!(content.contains(&format!("sandbox_mode = \"{sandbox}\"")));
        assert!(content.contains("developer_instructions"));
    }
}

#[test]
fn init_writes_omp_native_resources_only_when_explicitly_selected() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy();

    let result = run_init(&cwd, Some("omp"));

    assert!(result.ok);
    assert!(dir.path().join(".omp/skills/sdd-harness/SKILL.md").exists());
    assert!(dir.path().join(".omp/agents/sdd-worker.md").exists());
    assert!(!dir
        .path()
        .join(".agents/skills/sdd-harness/SKILL.md")
        .exists());
    let config = sdd_core::state::RuntimeStore::new(cwd.to_string())
        .read()
        .unwrap()
        .config;
    assert_eq!(config["hostAdapter"], "omp");
}

#[test]
fn config_store_rejects_obsolete_fields_and_versions() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy();
    run_init(&cwd, Some("omp"));

    let error = sdd_core::state::RuntimeStore::new(cwd.to_string())
        .update(|runtime| {
            runtime.config["schemaVersion"] = serde_json::json!(2);
            runtime.config["plugins"] = serde_json::json!({ "opencode": { "enabled": true } });
        })
        .unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[test]
fn init_is_idempotent_and_reports_overwritten_codex_template() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy();
    let first = run_init(&cwd, Some("codex"));
    assert!(first
        .warnings
        .as_ref()
        .unwrap()
        .iter()
        .any(|warning| warning.code == "W_ADAPTER_FILE"));

    let second = run_init(&cwd, Some("codex"));
    assert!(second.warnings.as_ref().is_none_or(|warnings| warnings
        .iter()
        .all(|warning| warning.code != "W_ADAPTER_FILE")));

    let skill_path = dir.path().join(".agents/skills/sdd-harness/SKILL.md");
    std::fs::write(&skill_path, "本地修改").unwrap();
    let refreshed = run_init(&cwd, Some("codex"));
    assert!(refreshed
        .warnings
        .as_ref()
        .unwrap()
        .iter()
        .any(|warning| warning.code == "W_ADAPTER_OVERWRITE"));
    assert!(std::fs::read_to_string(skill_path)
        .unwrap()
        .contains("# SDD Harness"));
}

#[test]
fn init_rejects_removed_or_invalid_host_adapter_values() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();

    for value in [
        serde_json::json!("opencode"),
        serde_json::json!("claude"),
        serde_json::json!("Codex"),
        serde_json::json!("OMP"),
        serde_json::json!(" codex"),
        serde_json::json!(["codex"]),
    ] {
        let result = sdd_core::run(&CommandRequest {
            command: "init".into(),
            cwd: cwd.clone(),
            args: Some(serde_json::json!({ "hostAdapter": value })),
        });
        assert_eq!(result.unwrap_err().code, "E_INVALID_PHASE_COMMAND");
    }

    let removed_adapter = sdd_core::run(&CommandRequest {
        command: "init".into(),
        cwd,
        args: Some(serde_json::json!({ "agent": "opencode" })),
    });
    assert_eq!(removed_adapter.unwrap_err().code, "E_INVALID_PHASE_COMMAND");
}

#[cfg(unix)]
#[test]
fn init_rejects_symlinked_agent_asset_paths() {
    use std::os::unix::fs::symlink;

    let target_project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(target_project.path().join(".codex/agents")).unwrap();
    let external_file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(external_file.path(), "外部内容").unwrap();
    symlink(
        external_file.path(),
        target_project.path().join(".codex/agents/sdd-worker.toml"),
    )
    .unwrap();
    let error = sdd_core::run(&CommandRequest {
        command: "init".into(),
        cwd: target_project.path().to_string_lossy().into_owned(),
        args: None,
    })
    .unwrap_err();
    assert_eq!(error.code, "E_SECURITY_BLOCKED");
    assert_eq!(
        std::fs::read_to_string(external_file.path()).unwrap(),
        "外部内容"
    );

    let parent_project = tempfile::tempdir().unwrap();
    let external_dir = tempfile::tempdir().unwrap();
    symlink(external_dir.path(), parent_project.path().join(".codex")).unwrap();
    let error = sdd_core::run(&CommandRequest {
        command: "init".into(),
        cwd: parent_project.path().to_string_lossy().into_owned(),
        args: None,
    })
    .unwrap_err();
    assert_eq!(error.code, "E_SECURITY_BLOCKED");
    assert!(!external_dir.path().join("agents/sdd-worker.toml").exists());
}
