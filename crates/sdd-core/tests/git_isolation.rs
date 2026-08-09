//! Git worktree 隔离测试。

use std::process::Command;

use sdd_core::git::GitIsolationManager;
use sdd_core::{contracts::CommandRequest, run};

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ensure_worktree_creates_and_reuses_registered_branch() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "test"]);
    std::fs::write(dir.path().join("README.md"), "demo").unwrap();
    git(dir.path(), &["add", "README.md"]);
    git(dir.path(), &["commit", "-qm", "base"]);

    let cwd = dir.path().to_string_lossy().to_string();
    let first = GitIsolationManager::ensure_worktree(&cwd, "change-1").unwrap();
    assert!(std::path::Path::new(&first.worktree_path).exists());
    assert_eq!(first.branch, "sdd/change-1");
    let second = GitIsolationManager::ensure_worktree(&cwd, "change-1").unwrap();
    assert_eq!(first, second);
}

#[test]
fn invalid_change_id_is_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let err =
        GitIsolationManager::ensure_worktree(dir.path().to_string_lossy().as_ref(), "../outside")
            .unwrap_err();
    assert_eq!(err.code, "E_SECURITY_BLOCKED");
}

#[test]
fn corrupted_config_does_not_silently_disable_isolation() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".sdd")).unwrap();
    std::fs::write(dir.path().join(".sdd/config.json"), "{").unwrap();
    let error = GitIsolationManager::enabled(dir.path().to_string_lossy().as_ref()).unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[test]
fn new_records_isolated_business_workspace_when_enabled() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "test"]);
    std::fs::write(dir.path().join("README.md"), "demo").unwrap();
    git(dir.path(), &["add", "README.md"]);
    git(dir.path(), &["commit", "-qm", "base"]);
    let cwd = dir.path().to_string_lossy().to_string();
    run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    let config_path = dir.path().join(".sdd/config.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    config["workflow"]["gitIsolation"] = serde_json::json!(true);
    std::fs::write(config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
    run(&CommandRequest {
        command: "new".into(),
        cwd: cwd.clone(),
        args: Some(serde_json::json!({
            "requirement": "授权用户通过 API 取消待处理订单，未授权请求必须拒绝，成功后写审计日志并由自动化测试覆盖"
        })),
    })
    .unwrap();
    let state = sdd_core::state::StateStore::new(cwd).read().unwrap();
    let workspace = state.workspace.expect("应记录隔离工作区");
    assert!(workspace.branch_name.unwrap().starts_with("sdd/change-"));
    assert!(std::path::Path::new(&workspace.worktree_path.unwrap()).exists());
}
