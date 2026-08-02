//! CLI 冒烟测试：参数解析、退出码与未初始化行为。

use std::process::Command;

fn sdd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sdd"))
}

#[test]
fn unknown_command_exits_with_code_2() {
    let out = sdd().arg("not-a-command").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn no_command_shows_help_and_exits_0() {
    let out = sdd().output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("sdd"));
}

#[test]
fn build_next_on_uninitialized_reports_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let out = sdd()
        .current_dir(dir.path())
        .args(["build", "next"])
        .output()
        .unwrap();
    // 未初始化 → 非 0 退出码（E_NOT_INITIALIZED=3）
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn version_flag_prints_version() {
    let out = sdd().arg("--version").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("sdd"));
}
