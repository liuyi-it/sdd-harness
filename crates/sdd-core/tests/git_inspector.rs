//! git 检查层测试。

use sdd_core::git::GitInspector;
use std::process::Command;

fn git(dir: &std::path::Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} 失败: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn init_repo(dir: &std::path::Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@test.test"]);
    git(dir, &["config", "user.name", "test"]);
}

#[test]
fn not_a_git_repo_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let err = GitInspector::snapshot(&cwd).unwrap_err();
    assert_eq!(err.code, "E_PATH_OUTSIDE_REPO");
}

#[test]
fn delta_detects_created_modified() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("a.txt"), "1").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    let base = GitInspector::snapshot(dir.path().to_string_lossy().as_ref()).unwrap();
    std::fs::write(dir.path().join("b.txt"), "2").unwrap();
    std::fs::write(dir.path().join("a.txt"), "2").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "change"]);
    let delta =
        GitInspector::compute_delta(dir.path().to_string_lossy().as_ref(), &base.head).unwrap();
    assert!(delta.iter().any(|d| d.path == "b.txt" && d.status == "A"));
    assert!(delta.iter().any(|d| d.path == "a.txt" && d.status == "M"));
}

#[test]
fn changed_files_reports_uncommitted() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("a.txt"), "1").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(dir.path().join("a.txt"), "2").unwrap();
    let files = GitInspector::changed_files(dir.path().to_string_lossy().as_ref()).unwrap();
    assert!(files.iter().any(|f| f.contains("a.txt")));
}

#[test]
fn workspace_fingerprint_changes_with_business_content() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("a.txt"), "1").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    let cwd = dir.path().to_string_lossy();
    let before = GitInspector::workspace_fingerprint(&cwd).unwrap();
    std::fs::write(dir.path().join("a.txt"), "2").unwrap();
    let after = GitInspector::workspace_fingerprint(&cwd).unwrap();
    assert_ne!(before, after);
}

#[test]
fn file_at_head_distinguishes_tracked_and_untracked_files() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("a.txt"), "base").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(dir.path().join("a.txt"), "current").unwrap();
    std::fs::write(dir.path().join("b.txt"), "new").unwrap();
    let cwd = dir.path().to_string_lossy();
    assert_eq!(
        GitInspector::file_at_head(&cwd, "a.txt")
            .unwrap()
            .as_deref(),
        Some("base")
    );
    assert_eq!(GitInspector::file_at_head(&cwd, "b.txt").unwrap(), None);
}

#[test]
fn path_within_repo_checks_scope() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let cwd = dir.path().to_string_lossy().to_string();
    assert!(GitInspector::path_within_repo(&cwd, "src/lib.rs"));
    assert!(!GitInspector::path_within_repo(&cwd, "../outside.txt"));
    assert!(!GitInspector::path_within_repo(&cwd, "..\\outside.txt"));
    assert!(!GitInspector::path_within_repo(&cwd, "/tmp/outside.txt"));
}

#[test]
fn resolve_repo_path_rejects_windows_absolute_and_unc_forms() {
    // 纯字符串规则：Windows 盘符 / UNC / \\?\ 前缀在任何平台都拒绝
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    for outside in [
        "C:\\foo",        // 盘符反斜杠
        "C:/foo",         // 盘符正斜杠
        "//server/share", // UNC
        "\\\\?\\C:\\foo", // \\?\ 长路径前缀
    ] {
        let err = GitInspector::resolve_repo_path(&cwd, outside).unwrap_err();
        assert_eq!(err.code, "E_PATH_OUTSIDE_REPO", "路径 {outside} 应被拒绝");
    }
    // 合法仓库相对路径不受影响
    assert!(GitInspector::resolve_repo_path(&cwd, "src/lib.rs").is_ok());
}

#[cfg(unix)]
#[test]
fn path_within_repo_rejects_symlink_escape() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let err = GitInspector::resolve_repo_path(&cwd, "link/result.json").unwrap_err();
    assert_eq!(err.code, "E_SYMLINK_BLOCKED");
}
