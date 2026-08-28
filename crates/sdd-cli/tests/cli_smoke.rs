use std::process::Command;

fn sdd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sdd"))
}

#[test]
fn help_lists_only_the_staged_commands() {
    let output = sdd().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in [
        "init", "status", "new", "change", "design", "plan", "build", "verify", "archive",
        "codebase",
    ] {
        assert!(stdout.contains(command), "帮助缺少 {command}: {stdout}");
    }
    assert!(!stdout.contains(" auto"));
    assert!(!stdout.contains(" review"));
}

#[test]
fn removed_commands_are_rejected_by_clap() {
    for command in ["auto", "review"] {
        let output = sdd().arg(command).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}

#[test]
fn result_json_options_are_registered_for_agent_phases() {
    for command in ["new", "change", "design", "plan", "verify"] {
        let output = sdd().args([command, "--help"]).output().unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("--result-json"), "{command}: {stdout}");
    }
}

#[test]
fn init_defaults_to_codex_and_installs_all_skills() {
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
    for command in [
        "harness", "init", "status", "new", "change", "design", "plan", "build", "verify",
        "archive", "codebase",
    ] {
        assert!(
            dir.path()
                .join(format!(".agents/skills/sdd-{command}/SKILL.md"))
                .is_file(),
            "缺少 sdd-{command}"
        );
    }
}

#[test]
fn global_change_is_available_to_every_stage() {
    for command in ["status", "design", "plan", "verify", "archive"] {
        let output = sdd()
            .args(["--change", "demo", command, "--json"])
            .output()
            .unwrap();
        assert_ne!(output.status.code(), Some(2), "{command} 未接受 --change");
    }
}
