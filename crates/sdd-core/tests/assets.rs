use sdd_core::contracts::{CommandRequest, HostAdapter};

const SKILLS: [&str; 5] = ["spec", "plan", "build", "verify", "archive"];
const OMP_COMMANDS: [&str; 12] = [
    "sdd.md",
    "sdd.init.md",
    "sdd.status.md",
    "sdd.spec.md",
    "sdd.new.md",
    "sdd.change.md",
    "sdd.design.md",
    "sdd.plan.md",
    "sdd.build.md",
    "sdd.verify.md",
    "sdd.archive.md",
    "sdd.codebase.md",
];

fn init(dir: &tempfile::TempDir, adapter: HostAdapter) {
    sdd_core::run(&CommandRequest {
        command: "init".to_string(),
        cwd: dir.path().to_string_lossy().into_owned(),
        args: Some(serde_json::json!({ "hostAdapter": adapter.as_str() })),
    })
    .unwrap();
}

fn skill_names(root: &std::path::Path) -> Vec<String> {
    let mut names = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn codex_installs_only_the_five_stage_skills() {
    let dir = tempfile::tempdir().unwrap();
    init(&dir, HostAdapter::Codex);
    let root = dir.path().join(".agents/skills");
    assert_eq!(
        skill_names(&root),
        [
            "sdd-archive",
            "sdd-build",
            "sdd-plan",
            "sdd-spec",
            "sdd-verify"
        ]
    );
    for skill in SKILLS {
        assert!(
            root.join(format!("sdd-{skill}/SKILL.md")).is_file(),
            "缺少 Codex Skill: {skill}"
        );
    }
    assert!(!dir.path().join(".codex/agents").exists());
}

#[test]
fn omp_installs_the_same_five_skills_and_all_command_entries() {
    let dir = tempfile::tempdir().unwrap();
    init(&dir, HostAdapter::Omp);
    let skills = dir.path().join(".omp/skills");
    assert_eq!(
        skill_names(&skills),
        [
            "sdd-archive",
            "sdd-build",
            "sdd-plan",
            "sdd-spec",
            "sdd-verify"
        ]
    );
    for skill in SKILLS {
        assert!(
            skills.join(format!("sdd-{skill}/SKILL.md")).is_file(),
            "缺少 OMP Skill: {skill}"
        );
    }
    let commands = dir.path().join(".omp/commands");
    for command in OMP_COMMANDS {
        assert!(
            commands.join(command).is_file(),
            "缺少 OMP command: {command}"
        );
    }
}

#[test]
fn omp_commands_route_without_deleted_skill_references() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for command in OMP_COMMANDS {
        let source =
            std::fs::read_to_string(root.join("assets/adapters/omp/commands").join(command))
                .unwrap();
        for deleted in [
            "sdd-harness",
            "sdd-init",
            "sdd-status",
            "sdd-new",
            "sdd-change",
            "sdd-design",
            "sdd-codebase",
        ] {
            assert!(
                !source.contains(deleted),
                "{command} 仍引用已删除 Skill {deleted}"
            );
        }
    }
}

#[test]
fn spec_skill_is_installed_and_refreshed_from_the_current_template() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (adapter, source, target) in [
        (
            HostAdapter::Codex,
            "assets/adapters/codex/skills/sdd-spec/SKILL.md",
            ".agents/skills/sdd-spec/SKILL.md",
        ),
        (
            HostAdapter::Omp,
            "assets/adapters/omp/skills/sdd-spec/SKILL.md",
            ".omp/skills/sdd-spec/SKILL.md",
        ),
    ] {
        let expected = std::fs::read_to_string(root.join(source)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let installed = dir.path().join(target);
        init(&dir, adapter);
        assert_eq!(std::fs::read_to_string(&installed).unwrap(), expected);

        std::fs::write(&installed, "# 旧规格模板\n").unwrap();
        init(&dir, adapter);
        assert_eq!(std::fs::read_to_string(&installed).unwrap(), expected);
    }
}
