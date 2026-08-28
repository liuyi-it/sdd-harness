use sdd_core::contracts::{CommandRequest, HostAdapter};

const COMMANDS: [&str; 10] = [
    "init", "status", "new", "change", "design", "plan", "build", "verify", "archive", "codebase",
];

fn init(dir: &tempfile::TempDir, adapter: HostAdapter) {
    sdd_core::run(&CommandRequest {
        command: "init".to_string(),
        cwd: dir.path().to_string_lossy().into_owned(),
        args: Some(serde_json::json!({ "hostAdapter": adapter.as_str() })),
    })
    .unwrap();
}

#[test]
fn codex_installs_orchestrator_and_every_command_skill() {
    let dir = tempfile::tempdir().unwrap();
    init(&dir, HostAdapter::Codex);
    assert!(dir
        .path()
        .join(".agents/skills/sdd-harness/SKILL.md")
        .is_file());
    for command in COMMANDS {
        let path = dir
            .path()
            .join(format!(".agents/skills/sdd-{command}/SKILL.md"));
        assert!(path.is_file(), "缺少 Codex Skill: {}", path.display());
    }
    assert!(!dir.path().join(".codex/agents").exists());
}

#[test]
fn omp_installs_every_skill_and_explicit_command() {
    let dir = tempfile::tempdir().unwrap();
    init(&dir, HostAdapter::Omp);
    assert!(dir
        .path()
        .join(".omp/skills/sdd-harness/SKILL.md")
        .is_file());
    assert!(dir.path().join(".omp/commands/sdd.md").is_file());
    for command in COMMANDS {
        let skill = dir
            .path()
            .join(format!(".omp/skills/sdd-{command}/SKILL.md"));
        let command_file = dir.path().join(format!(".omp/commands/sdd.{command}.md"));
        assert!(skill.is_file(), "缺少 OMP Skill: {}", skill.display());
        assert!(
            command_file.is_file(),
            "缺少 OMP command: {}",
            command_file.display()
        );
    }
    assert!(!dir.path().join(".omp/commands/sdd.review.md").exists());
}

#[test]
fn stage_skills_require_user_selection_when_multiple_changes_exist() {
    for path in [
        "assets/adapters/codex/skills/sdd-harness/SKILL.md",
        "assets/adapters/codex/skills/sdd-design/SKILL.md",
        "assets/adapters/omp/skills/sdd-harness/SKILL.md",
        "assets/adapters/omp/skills/sdd-design/SKILL.md",
    ] {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(path),
        )
        .unwrap();
        assert!(source.contains("多个") && source.contains("询问"), "{path}");
    }
}
