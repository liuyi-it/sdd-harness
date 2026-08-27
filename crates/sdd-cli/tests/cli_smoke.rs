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
        .args(["--host-adapter", "omp"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("INDEX_READY"), "缺少 INDEX_READY: {stdout}");
    assert!(stdout.contains("changeId") || stdout.contains("\"next\""));
}

#[test]
fn init_help_hides_host_selection_parameter() {
    let out = sdd().args(["init", "--help"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(!help.contains("--agent"));
    assert!(!help.contains("--host-adapter"));
}

#[test]
fn init_codex_writes_native_project_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let out = sdd()
        .current_dir(dir.path())
        .args(["init", "--host-adapter", "codex", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
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
    assert!(!dir.path().join(".omp/skills/sdd-harness/SKILL.md").exists());
}

#[test]
fn init_without_host_marker_defaults_to_codex() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let out = sdd()
        .current_dir(dir.path())
        .args(["init", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(dir
        .path()
        .join(".agents/skills/sdd-harness/SKILL.md")
        .exists());
}

#[test]
fn init_rejects_new_only_non_interactive_flag() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let output = sdd()
        .current_dir(dir.path())
        .args(["init", "--non-interactive", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!dir.path().join(".sdd/runtime.json").exists());
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
fn removed_legacy_cli_aliases_are_rejected() {
    for args in [
        vec!["status", "--loop"],
        vec!["status", "--loop-status"],
        vec!["init", "--structure-policy", "free-design"],
    ] {
        let out = sdd().args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}

#[test]
fn install_surface_only_exposes_sdd() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // 安装脚本不得暴露旧命令入口或旧 npm 清理变量。
    let install = std::fs::read_to_string(root.join("scripts/install.sh")).unwrap();
    assert!(
        !install.contains("sdd-harness"),
        "install.sh 暴露旧命令入口"
    );
    assert!(
        !install.contains("LEGACY_NPM_PACKAGES"),
        "install.sh 保留旧 npm 清理"
    );
    // 卸载脚本只处理当前二进制，不保留 npm 或旧命令兼容清理。
    let uninstall = std::fs::read_to_string(root.join("scripts/uninstall.sh")).unwrap();
    assert!(
        !uninstall.contains("npm uninstall"),
        "uninstall.sh 不应保留历史 npm 清理"
    );
    assert!(
        !uninstall.contains("sdd-harness"),
        "uninstall.sh 保留旧命令兼容清理"
    );
    assert!(
        !uninstall.contains("install -m"),
        "uninstall.sh 不应注册新命令"
    );
}

#[test]
fn malformed_values_exit_with_code_2() {
    for args in [
        vec!["status", "--timeout", "nope"],
        vec!["new", "需求", "--answers", "{broken"],
        vec!["auto", "--events", "--tail", "nope"],
        vec!["auto", "--answers", "{broken"],
        vec!["init", "--structurePolicy", "invalid"],
        vec!["init", "--agent", "opencode"],
        vec!["plan", "--dependencies", "{}"],
        vec!["build", "unknown"],
        vec!["codebase", "unknown"],
    ] {
        let out = sdd().args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}

#[test]
fn change_rejects_conflicting_global_and_positional_ids() {
    let out = sdd()
        .args(["--change", "change-a", "change", "change-b", "新需求"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("不得同时传入全局 --change"));
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

#[test]
fn text_mode_omits_long_data() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let out = sdd()
        .current_dir(dir.path())
        .args(["init", "--host-adapter", "omp"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    // 完整需求生成的规格模型（约 1200 字符）远超 512：文本模式应提示省略而非倾倒 JSON
    let requirement = "授权用户通过 POST /orders/{id}/cancel 请求取消待处理订单，入参 order_id，返回 status 和 error_code，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";
    let out = sdd()
        .current_dir(dir.path())
        .args(["new", requirement])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("SPEC_READY"),
        "完整需求应直接生成规格"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("数据：<JSON 过长，已省略"),
        "应提示长数据已省略: {stdout}"
    );
    assert!(
        stdout.contains("使用 --json 查看完整内容"),
        "应提示使用 --json: {stdout}"
    );
}

#[test]
fn text_mode_error_contains_error_code() {
    let dir = tempfile::tempdir().unwrap();
    let out = sdd()
        .current_dir(dir.path())
        .args(["build", "next"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("错误（E_NOT_INITIALIZED）"),
        "文本错误应包含错误码: {stderr}"
    );
}

#[test]
fn auto_tail_without_events_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let init = sdd()
        .current_dir(dir.path())
        .args(["init", "--host-adapter", "omp"])
        .output()
        .unwrap();
    assert_eq!(init.status.code(), Some(0));
    let out = sdd()
        .current_dir(dir.path())
        .args(["auto", "--tail", "5"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--tail 必须与 --events 一起使用"),
        "缺少 --events 时应报错: {stderr}"
    );
}
