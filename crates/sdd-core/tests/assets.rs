use sdd_core::contracts::{CommandRequest, HostAdapter};

const SKILLS: [&str; 5] = ["spec", "plan", "build", "verify", "archive"];
const OMP_COMMANDS: [&str; 10] = [
    "sdd.md",
    "sdd.init.md",
    "sdd.status.md",
    "sdd.spec.md",
    "sdd.change.md",
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
fn all_skills_and_local_references_are_installed_and_refreshed() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rules = std::fs::read_to_string(root.join("assets/policies/workspace.md")).unwrap();
    let collaboration =
        std::fs::read_to_string(root.join("assets/policies/collaboration.md")).unwrap();
    let mut collaboration_references = 0;
    for (adapter, host, target) in [
        (HostAdapter::Codex, "codex", ".agents/skills"),
        (HostAdapter::Omp, "omp", ".omp/skills"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        init(&dir, adapter);
        for skill in SKILLS {
            let installed = dir.path().join(target).join(format!("sdd-{skill}"));
            let source = root.join(format!(
                "assets/adapters/{host}/skills/sdd-{skill}/SKILL.md"
            ));
            assert_eq!(
                std::fs::read_to_string(installed.join("SKILL.md")).unwrap(),
                std::fs::read_to_string(source).unwrap()
            );
            assert_eq!(
                std::fs::read_to_string(installed.join("references/workspace.md")).unwrap(),
                rules
            );
            assert_eq!(
                std::fs::read_to_string(installed.join("references/collaboration.md")).unwrap(),
                collaboration
            );
            collaboration_references += 1;
            std::fs::write(installed.join("SKILL.md"), "# 旧模板\n").unwrap();
            std::fs::write(installed.join("references/workspace.md"), "# 旧规则\n").unwrap();
            std::fs::write(
                installed.join("references/collaboration.md"),
                "# 旧协作规则\n",
            )
            .unwrap();
        }
        assert_eq!(
            std::fs::read_dir(dir.path().join(target))
                .unwrap()
                .flat_map(|entry| {
                    let skill = entry.unwrap().path();
                    std::fs::read_dir(skill.join("references")).unwrap()
                })
                .filter(|entry| {
                    entry.as_ref().unwrap().file_name().to_string_lossy() == "collaboration.md"
                })
                .count(),
            SKILLS.len()
        );
        init(&dir, adapter);
        for skill in SKILLS {
            let installed = dir.path().join(target).join(format!("sdd-{skill}"));
            let expected = std::fs::read_to_string(root.join(format!(
                "assets/adapters/{host}/skills/sdd-{skill}/SKILL.md"
            )))
            .unwrap();
            let content = std::fs::read_to_string(installed.join("SKILL.md")).unwrap();
            assert_eq!(content, expected);
            assert_eq!(
                std::fs::read_to_string(installed.join("references/collaboration.md")).unwrap(),
                collaboration
            );
            // 校验实际下发模板中的本地引用可达，不以提示关键词证明模型行为。
            let reference = content
                .split("](references/")
                .nth(1)
                .expect("缺少本地参考链接")
                .split(')')
                .next()
                .unwrap();
            let resolved = installed
                .join("references")
                .join(reference)
                .canonicalize()
                .unwrap();
            assert!(resolved.starts_with(installed.canonicalize().unwrap()));
            assert_eq!(std::fs::read_to_string(resolved).unwrap(), rules);
        }
    }
    assert_eq!(collaboration_references, 10);
}

#[cfg(unix)]
#[test]
fn shared_reference_symlink_cannot_overwrite_an_external_file() {
    for (adapter, target) in [
        (HostAdapter::Codex, ".agents/skills"),
        (HostAdapter::Omp, ".omp/skills"),
    ] {
        for reference_name in ["workspace.md", "collaboration.md"] {
            let dir = tempfile::tempdir().unwrap();
            let external = tempfile::tempdir().unwrap();
            let victim = external.path().join(reference_name);
            std::fs::write(&victim, "外部用户文件").unwrap();
            let reference = dir.path().join(target).join("sdd-plan/references");
            std::fs::create_dir_all(&reference).unwrap();
            std::os::unix::fs::symlink(&victim, reference.join(reference_name)).unwrap();
            let error = sdd_core::run(&CommandRequest {
                command: "init".to_string(),
                cwd: dir.path().to_string_lossy().into_owned(),
                args: Some(serde_json::json!({"hostAdapter": adapter.as_str()})),
            })
            .unwrap_err();
            assert_eq!(error.code, "E_SECURITY_BLOCKED");
            assert_eq!(std::fs::read_to_string(victim).unwrap(), "外部用户文件");
        }
    }
}
