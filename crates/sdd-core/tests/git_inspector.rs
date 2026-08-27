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
    assert!(!GitInspector::is_git_repo(&cwd).unwrap());
    let err = GitInspector::head(&cwd).unwrap_err();
    assert_eq!(err.code, "E_PATH_OUTSIDE_REPO");
}

#[test]
fn bare_repository_is_not_a_working_tree() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--bare", "-q"]);
    assert!(!GitInspector::is_git_repo(&dir.path().to_string_lossy()).unwrap());
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
fn changed_files_reports_both_sides_of_a_rename() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("old.txt"), "content").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    git(dir.path(), &["mv", "old.txt", "new.txt"]);

    let files = GitInspector::changed_files(&dir.path().to_string_lossy()).unwrap();

    assert_eq!(files, ["new.txt", "old.txt"]);
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

#[cfg(unix)]
#[test]
fn file_hashes_use_symlink_text_instead_of_target_content() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("target.txt"), "before").unwrap();
    std::os::unix::fs::symlink("target.txt", dir.path().join("link.txt")).unwrap();
    let cwd = dir.path().to_string_lossy();
    let files = vec!["link.txt".to_string()];

    let before = GitInspector::file_hashes(&cwd, &files).unwrap();
    std::fs::write(dir.path().join("target.txt"), "after").unwrap();
    let after = GitInspector::file_hashes(&cwd, &files).unwrap();

    assert_eq!(before, after, "链接目标内容不应改变 symlink 自身摘要");
}

#[test]
fn resolve_repo_path_checks_scope() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    let cwd = dir.path().to_string_lossy().to_string();
    assert!(GitInspector::resolve_repo_path(&cwd, "src/lib.rs").is_ok());
    assert!(GitInspector::resolve_repo_path(&cwd, "../outside.txt").is_err());
    assert!(GitInspector::resolve_repo_path(&cwd, "..\\outside.txt").is_err());
    assert!(GitInspector::resolve_repo_path(&cwd, "/tmp/outside.txt").is_err());
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

    let dangling_target = outside.path().join("missing.txt");
    std::os::unix::fs::symlink(&dangling_target, dir.path().join("dangling.txt")).unwrap();
    let err = GitInspector::resolve_repo_path(&cwd, "dangling.txt").unwrap_err();
    assert_eq!(err.code, "E_SYMLINK_BLOCKED");
}
