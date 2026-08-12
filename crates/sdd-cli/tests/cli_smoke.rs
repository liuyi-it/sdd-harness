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
fn json_error_outputs_command_result() {
    let dir = tempfile::tempdir().unwrap();
    let out = sdd()
        .current_dir(dir.path())
        .args(["build", "next", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stderr.is_empty(), "JSON 模式不应写 stderr");
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(result["exitCode"], 3);
    assert_eq!(result["error"]["code"], "E_NOT_INITIALIZED");
}

#[test]
fn version_flag_prints_version() {
    let out = sdd().arg("--version").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("sdd"));
}

#[test]
fn status_json_outputs_command_result() {
    let dir = tempfile::tempdir().unwrap();
    // 未初始化：status 应输出 JSON（含 exitCode）且退出码 0
    let out = sdd()
        .current_dir(dir.path())
        .args(["status", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("exitCode"), "缺少 exitCode: {stdout}");
    assert!(stdout.contains("NOT_INITIALIZED"));
}

#[test]
fn init_json_outputs_state() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let out = sdd()
        .current_dir(dir.path())
        .args(["init", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("INDEX_READY"), "缺少 INDEX_READY: {stdout}");
    assert!(stdout.contains("changeId") || stdout.contains("\"next\""));
}

#[test]
fn text_output_readable() {
    let dir = tempfile::tempdir().unwrap();
    let out = sdd()
        .current_dir(dir.path())
        .arg("status")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    // 文本模式包含状态与下一步
    assert!(stdout.contains("状态") || stdout.contains("sdd init"));
}

#[test]
fn status_loop_flag_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let out = sdd()
        .current_dir(dir.path())
        .args(["status", "--loop", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn malformed_values_exit_with_code_2() {
    for args in [
        vec!["status", "--timeout", "nope"],
        vec!["new", "需求", "--answers", "{broken"],
        vec!["auto", "--events", "--tail", "nope"],
        vec!["auto", "--answers", "{broken"],
        vec!["init", "--structurePolicy", "invalid"],
        vec!["plan", "--dependencies", "{}"],
    ] {
        let out = sdd().args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}

#[test]
fn review_help_is_available() {
    let out = sdd().args(["review", "--help"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("review"),
        "帮助应包含 review 命令: {stdout}"
    );
}

#[test]
fn auto_help_includes_answers_option() {
    let out = sdd().args(["auto", "--help"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--answers"),
        "帮助应包含 --answers: {stdout}"
    );
}

#[test]
fn change_help_includes_answers_option() {
    let out = sdd().args(["change", "--help"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--answers"));
}
