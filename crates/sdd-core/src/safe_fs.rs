//! 受管文件的安全读写原语：拒绝符号链接，并以唯一临时文件原子提交。

#[cfg(unix)]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::error::SddError;

pub(crate) fn reject_symlink(path: &Path, label: &str) -> Result<(), SddError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(SddError::new(
            "E_SYMLINK_BLOCKED",
            &format!("{label} 不得是符号链接：{}", path.display()),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("检查 {label} 失败：{error}"),
        )),
    }
}

/// 以同目录唯一临时文件提交内容，避免跟随既有符号链接并保证目录项落盘。
pub(crate) fn atomic_write(path: &Path, content: &[u8], label: &str) -> Result<(), SddError> {
    reject_symlink(path, label)?;
    let parent = path
        .parent()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", &format!("{label} 缺少父目录")))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", &format!("{label} 文件名无效")))?;
    let temp = parent.join(format!(
        ".{file_name}.{}.tmp",
        crate::state::state_store::unique_id("write")?
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| {
                SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("创建 {label} 临时文件失败：{error}"),
                )
            })?;
        file.write_all(content).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("写入 {label} 临时文件失败：{error}"),
            )
        })?;
        file.sync_all().map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("同步 {label} 临时文件失败：{error}"),
            )
        })?;
        fs::rename(&temp, path).map_err(|error| {
            SddError::new("E_STATE_CORRUPTED", &format!("提交 {label} 失败：{error}"))
        })?;
        sync_dir(parent)
    })();
    if result.is_err() {
        match fs::remove_file(&temp) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(cleanup_error) => {
                let mut error = result.expect_err("已确认原子写失败");
                error.message = format!(
                    "{}；清理临时文件 {} 失败：{}",
                    error.message,
                    temp.display(),
                    cleanup_error
                );
                return Err(error);
            }
        }
    }
    result
}

/// 目录 fsync：保证 rename 后的目录项落盘（防断电丢文件）。
#[cfg(unix)]
fn sync_dir(dir: &Path) -> Result<(), SddError> {
    File::open(dir)
        .and_then(|dir_file| dir_file.sync_all())
        .map_err(|error| SddError::new("E_STATE_CORRUPTED", &format!("同步文件目录失败：{error}")))
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> Result<(), SddError> {
    Ok(())
}
