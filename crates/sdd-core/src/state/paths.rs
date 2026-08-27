//! `.sdd` 托管路径解析：拒绝符号链接并保证所有写入留在项目内。

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::SddError;

use super::state_store::SDD_DIR;

pub(crate) fn ensure_sdd_dir(root: &Path) -> Result<PathBuf, SddError> {
    resolve_sdd_dir(root, true)?
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "创建 .sdd 目录后仍无法解析"))
}

pub(crate) fn existing_sdd_dir(root: &Path) -> Result<Option<PathBuf>, SddError> {
    resolve_sdd_dir(root, false)
}

fn resolve_sdd_dir(root: &Path, create: bool) -> Result<Option<PathBuf>, SddError> {
    let root = root.canonicalize().map_err(|error| {
        SddError::new(
            "E_PATH_OUTSIDE_REPO",
            &format!("解析项目根目录失败：{error}"),
        )
    })?;
    let dir = root.join(SDD_DIR);
    match ensure_directory(&dir, create, "SDD 状态目录") {
        Ok(()) => {}
        Err(error) if !create && error.code == "E_MISSING_CHANGE" => return Ok(None),
        Err(error) => return Err(error),
    }
    let resolved = dir.canonicalize().map_err(|error| {
        SddError::new("E_STATE_CORRUPTED", &format!("解析 .sdd 目录失败：{error}"))
    })?;
    if !resolved.starts_with(&root) {
        return Err(SddError::new(
            "E_SYMLINK_BLOCKED",
            ".sdd 目录逃逸项目根目录",
        ));
    }
    Ok(Some(resolved))
}

pub(crate) fn changes_dir(cwd: &str, create: bool) -> Result<PathBuf, SddError> {
    managed_subdir(cwd, "changes", create, "变更根目录")
}

pub(crate) fn worktrees_dir(cwd: &str, create: bool) -> Result<PathBuf, SddError> {
    managed_subdir(cwd, "worktrees", create, "worktree 根目录")
}

fn managed_subdir(cwd: &str, name: &str, create: bool, label: &str) -> Result<PathBuf, SddError> {
    let root = Path::new(cwd);
    let sdd = if create {
        ensure_sdd_dir(root)?
    } else {
        existing_sdd_dir(root)?.ok_or_else(|| {
            SddError::new(
                "E_MISSING_CHANGE",
                &format!("SDD 状态目录不存在：{}", root.join(SDD_DIR).display()),
            )
        })?
    };
    let dir = sdd.join(name);
    ensure_directory(&dir, create, label)?;
    let resolved = dir.canonicalize().map_err(|error| {
        SddError::new("E_MISSING_CHANGE", &format!("解析 {label} 失败：{error}"))
    })?;
    if !resolved.starts_with(&sdd) {
        return Err(SddError::new(
            "E_SYMLINK_BLOCKED",
            &format!("{label} 逃逸 .sdd"),
        ));
    }
    Ok(resolved)
}

pub(crate) fn change_dir(cwd: &str, change_id: &str, create: bool) -> Result<PathBuf, SddError> {
    crate::git::isolation::validate_change_id(change_id)?;
    let changes = changes_dir(cwd, create)?;
    let dir = changes.join(change_id);
    ensure_directory(&dir, create, "变更目录")?;
    let resolved = dir.canonicalize().map_err(|error| {
        SddError::new(
            "E_MISSING_CHANGE",
            &format!("解析变更目录 {change_id} 失败：{error}"),
        )
    })?;
    if !resolved.starts_with(&changes) {
        return Err(SddError::new(
            "E_SYMLINK_BLOCKED",
            &format!("变更目录 {change_id} 逃逸 .sdd/changes"),
        ));
    }
    Ok(resolved)
}

fn ensure_directory(path: &Path, create: bool, label: &str) -> Result<(), SddError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(SddError::new(
            "E_SYMLINK_BLOCKED",
            &format!("{label} 不得是符号链接：{}", path.display()),
        )),
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("{label} 不是目录：{}", path.display()),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
            match fs::create_dir(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    // 首次并发命令可能同时观察到目录缺失；竞争后必须重新验证真实节点。
                    ensure_directory(path, false, label)
                }
                Err(error) => Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("创建 {label} 失败：{error}"),
                )),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(SddError::new(
            "E_MISSING_CHANGE",
            &format!("{label} 不存在：{}", path.display()),
        )),
        Err(error) => Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("检查 {label} 失败：{error}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_managed_path_lookup_does_not_create_sdd_directory() {
        let project = tempfile::tempdir().unwrap();
        let cwd = project.path().to_str().unwrap();

        let error = change_dir(cwd, "missing-change", false).unwrap_err();

        assert_eq!(error.code, "E_MISSING_CHANGE");
        assert!(!project.path().join(SDD_DIR).exists());
    }

    #[test]
    fn concurrent_first_lock_creation_is_race_safe() {
        let project = tempfile::tempdir().unwrap();
        let cwd = project.path().to_str().unwrap().to_string();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let mut threads = Vec::new();
        for index in 0..8 {
            let cwd = cwd.clone();
            let barrier = barrier.clone();
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                let _guard = crate::state::file_lock::lock_sdd(
                    &cwd,
                    &format!("test-{index}"),
                    None,
                    Some(5_000),
                )
                .unwrap();
            }));
        }

        for thread in threads {
            thread.join().unwrap();
        }
    }
}
