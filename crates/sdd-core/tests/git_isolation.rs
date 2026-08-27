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

#[cfg(unix)]
#[test]
fn ensure_worktree_rejects_symlinked_worktree_root() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "test"]);
    std::fs::write(dir.path().join("README.md"), "demo").unwrap();
    git(dir.path(), &["add", "README.md"]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::create_dir(dir.path().join(".sdd")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join(".sdd/worktrees")).unwrap();

    let error = GitIsolationManager::ensure_worktree(&dir.path().to_string_lossy(), "change-1")
        .unwrap_err();

    assert_eq!(error.code, "E_SYMLINK_BLOCKED");
    assert!(!outside.path().join("change-1").exists());
}

#[test]
fn corrupted_config_does_not_silently_disable_isolation() {
    let error = GitIsolationManager::enabled(&serde_json::json!({})).unwrap_err();
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
    sdd_core::state::RuntimeStore::new(cwd.clone())
        .update(|runtime| {
            runtime.config["workflow"]["gitIsolation"] = serde_json::json!(true);
        })
        .unwrap();
    run(&CommandRequest {
        command: "new".into(),
        cwd: cwd.clone(),
        args: Some(serde_json::json!({
            "requirement": "授权用户通过 POST /orders/{id}/cancel 取消待处理订单，入参 order_id，返回 status 和 error_code，未授权请求必须拒绝，成功后写审计日志并由自动化测试覆盖"
        })),
    })
    .unwrap();
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let workspace = state.workspace.expect("应记录隔离工作区");
    let branch = workspace.branch_name.unwrap();
    assert!(branch.starts_with("sdd/"));
    assert!(!branch.starts_with("sdd/change-"));
    assert!(std::path::Path::new(&workspace.worktree_path.unwrap()).exists());

    let outside = tempfile::tempdir().unwrap();
    let error = sdd_core::state::RuntimeStore::new(cwd.clone())
        .update(|runtime| {
            runtime.state.workspace.as_mut().unwrap().worktree_path =
                Some(outside.path().to_string_lossy().to_string());
        })
        .unwrap_err();
    assert_eq!(error.code, "E_SYMLINK_BLOCKED");

    let error = sdd_core::state::RuntimeStore::new(cwd)
        .update(|runtime| runtime.state.workspace = None)
        .unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");
}
