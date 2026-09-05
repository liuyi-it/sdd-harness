use std::process::Command;

fn sdd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sdd"))
}

#[test]
fn help_lists_the_unified_spec_commands() {
    let output = sdd().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in [
        "init", "status", "spec", "change", "plan", "build", "verify", "archive", "codebase",
    ] {
        assert!(stdout.contains(command), "帮助缺少 {command}: {stdout}");
    }
    for removed in ["new", "design", "auto", "review"] {
        assert!(
            !stdout.contains(&format!(" {removed}")),
            "帮助仍包含 {removed}: {stdout}"
        );
    }
}

#[test]
fn removed_commands_are_rejected_by_clap() {
    for command in ["new", "design", "auto", "review"] {
        let output = sdd().arg(command).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}

#[test]
fn result_json_options_are_registered_for_agent_phases() {
    for command in ["spec", "change", "plan", "verify"] {
        let output = sdd().args([command, "--help"]).output().unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("--result-json"), "{command}: {stdout}");
    }
}

#[test]
fn init_defaults_to_codex_and_installs_only_five_skills() {
    let dir = tempfile::tempdir().unwrap();
    let output = sdd()
        .args(["--cwd", &dir.path().to_string_lossy(), "init", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let skills = dir.path().join(".agents/skills");
    let mut names = std::fs::read_dir(&skills)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        [
            "sdd-archive",
            "sdd-build",
            "sdd-plan",
            "sdd-spec",
            "sdd-verify"
        ]
    );
}

#[test]
fn global_change_is_available_to_every_change_scoped_command() {
    for command in ["status", "spec", "change", "plan", "verify", "archive"] {
        let output = sdd()
            .args(["--change", "demo", command, "--json"])
            .output()
            .unwrap();
        assert_ne!(output.status.code(), Some(2), "{command} 未接受 --change");
    }
}
