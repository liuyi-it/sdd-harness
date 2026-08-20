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
    assert!(dir.path().join(".codex/agents/sdd-reviewer.toml").exists());
    assert!(!dir.path().join(".opencode").exists());

    let config = sdd_core::state::runtime_store::read_config(&cwd).unwrap();
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
    assert!(skill.contains("sdd-reviewer"));
    assert!(skill.contains("不要并行编辑共享文件"));

    for (file, name, sandbox) in [
        ("sdd-explorer.toml", "sdd-explorer", "read-only"),
        ("sdd-worker.toml", "sdd-worker", "workspace-write"),
        ("sdd-reviewer.toml", "sdd-reviewer", "read-only"),
    ] {
        let content = std::fs::read_to_string(dir.path().join(".codex/agents").join(file)).unwrap();
        assert!(content.contains(&format!("name = \"{name}\"")));
        assert!(content.contains("model_reasoning_effort"));
        assert!(content.contains(&format!("sandbox_mode = \"{sandbox}\"")));
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
    let config = sdd_core::state::runtime_store::read_config(&cwd).unwrap();
    assert_eq!(config["hostAdapter"], "omp");
}

#[test]
fn reinit_replaces_legacy_plugin_config_with_current_host_adapter() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy();
    run_init(&cwd, Some("omp"));

    let mut config = sdd_core::state::runtime_store::read_config(&cwd).unwrap();
    config["schemaVersion"] = serde_json::json!(2);
    config["plugins"] = serde_json::json!({ "opencode": { "enabled": true } });
    sdd_core::state::runtime_store::write_config(&cwd, config).unwrap();

    run_init(&cwd, Some("codex"));
    let config = sdd_core::state::runtime_store::read_config(&cwd).unwrap();
    assert_eq!(config["schemaVersion"], 3);
    assert_eq!(config["hostAdapter"], "codex");
    assert!(config.get("plugins").is_none());
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
        .any(|warning| warning["code"] == "W_ADAPTER_FILE"));

    let second = run_init(&cwd, Some("codex"));
    assert!(second.warnings.as_ref().is_none_or(|warnings| warnings
        .iter()
        .all(|warning| warning["code"] != "W_ADAPTER_FILE")));

    let skill_path = dir.path().join(".agents/skills/sdd-harness/SKILL.md");
    std::fs::write(&skill_path, "本地修改").unwrap();
    let refreshed = run_init(&cwd, Some("codex"));
    assert!(refreshed
        .warnings
        .as_ref()
        .unwrap()
        .iter()
        .any(|warning| warning["code"] == "W_ADAPTER_OVERWRITE"));
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
        serde_json::json!(["codex"]),
    ] {
        let result = sdd_core::run(&CommandRequest {
            command: "init".into(),
            cwd: cwd.clone(),
            args: Some(serde_json::json!({ "hostAdapter": value })),
        });
        assert_eq!(result.unwrap_err().code, "E_INVALID_PHASE_COMMAND");
    }

    let legacy = sdd_core::run(&CommandRequest {
        command: "init".into(),
        cwd,
        args: Some(serde_json::json!({ "agent": "opencode" })),
    });
    assert_eq!(legacy.unwrap_err().code, "E_INVALID_PHASE_COMMAND");
}
