//! 基于操作系统独占锁的 `.sdd` 写锁。
//!
//! 锁文件只保存当前持有者的诊断元数据；排他性由保持打开的文件描述符保证。
//! 因此进程异常退出会由操作系统自动释放锁，也不需要“过期抢占”或删除锁文件。

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use crate::error::SddError;
use crate::state::state_store::now_iso;

const LOCK_FILE: &str = "lock";
const RETRY_DELAY_MS: u64 = 50;
const MAX_LOCK_METADATA_BYTES: u64 = 8 * 1024;

thread_local! {
    /// 同一线程中的组合命令复用持有中的文件描述符，避免内部状态写入产生自竞争。
    /// 表中只存弱引用，不延长文件锁生命周期；其他线程和进程仍由 OS 独占锁阻断。
    static HELD_LOCKS: RefCell<HashMap<PathBuf, Weak<LockHandle>>> = RefCell::new(HashMap::new());
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct LockData {
    pid: u32,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    change_id: Option<String>,
    created_at: String,
}

#[derive(Debug)]
struct LockHandle {
    file: File,
}

impl Drop for LockHandle {
    fn drop(&mut self) {
        drop(self.file.unlock());
    }
}

/// 持有文件描述符即持有独占锁；同一线程的嵌套调用复用句柄，最后一个 guard Drop 时释放。
#[derive(Debug)]
pub struct SddLockGuard {
    path: PathBuf,
    _handle: Rc<LockHandle>,
}

impl Drop for SddLockGuard {
    fn drop(&mut self) {
        if Rc::strong_count(&self._handle) == 1 {
            HELD_LOCKS.with(|locks| {
                locks.borrow_mut().remove(&self.path);
            });
        }
    }
}

/// 获取写锁。`command` 为当前命令名（如 "sdd init"），`change_id` 可选。
/// `timeout_ms` 为 None 时不做等待（立即冲突则报 E_CONCURRENT_RUN）。
pub fn lock_sdd(
    cwd: &str,
    command: &str,
    change_id: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<SddLockGuard, SddError> {
    lock_named(cwd, LOCK_FILE, command, change_id, timeout_ms, true)
}

/// 获取已初始化项目的写锁；`.sdd` 不存在时直接失败且不创建任何状态目录。
pub(crate) fn lock_initialized_sdd(
    cwd: &str,
    command: &str,
    change_id: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<SddLockGuard, SddError> {
    lock_named(cwd, LOCK_FILE, command, change_id, timeout_ms, false)
}

/// auto 使用独立协调锁串行化整条 loop，同时允许内部命令继续获取 `.sdd/lock`。
pub(crate) fn lock_auto(
    cwd: &str,
    command: &str,
    change_id: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<SddLockGuard, SddError> {
    lock_named(cwd, "auto.lock", command, change_id, timeout_ms, false)
}

pub(crate) fn current_thread_holds_auto_lock(cwd: &str) -> Result<bool, SddError> {
    let Some(dir) = crate::state::paths::existing_sdd_dir(Path::new(cwd))? else {
        return Ok(false);
    };
    let path = dir.join("auto.lock");
    Ok(HELD_LOCKS.with(|locks| locks.borrow().get(&path).and_then(Weak::upgrade).is_some()))
}

fn lock_named(
    cwd: &str,
    file_name: &str,
    command: &str,
    change_id: Option<&str>,
    timeout_ms: Option<u64>,
    create_sdd: bool,
) -> Result<SddLockGuard, SddError> {
    let root = Path::new(cwd);
    let dir = if create_sdd {
        crate::state::paths::ensure_sdd_dir(root)?
    } else {
        crate::state::paths::existing_sdd_dir(root)?.ok_or_else(|| {
            SddError::new("E_NOT_INITIALIZED", "请先运行 sdd init 再执行其他命令")
                .with_next("sdd init")
        })?
    };
    let path = dir.join(file_name);
    crate::safe_fs::reject_symlink(&path, "SDD 锁文件")?;
    if let Some(guard) = reentrant_guard(&path) {
        return Ok(guard);
    }
    let deadline = timeout_ms
        .map(|milliseconds| {
            std::time::Instant::now()
                .checked_add(std::time::Duration::from_millis(milliseconds))
                .ok_or_else(|| SddError::new("E_INVALID_PHASE_COMMAND", "锁等待时间超出支持范围"))
        })
        .transpose()?;

    loop {
        match try_acquire(&path, command, change_id) {
            Ok(guard) => return Ok(guard),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if deadline.is_none() {
                    return Err(SddError::new("E_CONCURRENT_RUN", &holder_message(&path))
                        .with_next("sdd status"));
                }
                if let Some(at) = deadline {
                    let now = std::time::Instant::now();
                    if now < at {
                        std::thread::sleep(
                            (at - now).min(std::time::Duration::from_millis(RETRY_DELAY_MS)),
                        );
                        continue;
                    }
                }
                return Err(SddError::new(
                    "E_LOCK_TIMEOUT",
                    &format!("{}，无法在限定时间内获取写锁", holder_message(&path)),
                )
                .with_next("sdd status"));
            }
            Err(error) => {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("获取 .sdd 写锁失败：{error}"),
                ));
            }
        }
    }
}

fn try_acquire(
    path: &Path,
    command: &str,
    change_id: Option<&str>,
) -> Result<SddLockGuard, std::io::Error> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        // 先取得 OS 锁，再由 write_lock_data 截断并写入诊断元数据。
        // 打开时不可截断，否则竞争进程会抹掉当前持锁者的信息。
        .truncate(false)
        .open(path)?;
    file.try_lock()?;

    let owner = LockData {
        pid: std::process::id(),
        command: command.to_string(),
        change_id: change_id.map(str::to_string),
        created_at: now_iso(),
    };
    if let Err(error) = write_lock_data(&mut file, &owner) {
        drop(file.unlock());
        return Err(error);
    }
    let handle = Rc::new(LockHandle { file });
    HELD_LOCKS.with(|locks| {
        locks
            .borrow_mut()
            .insert(path.to_path_buf(), Rc::downgrade(&handle));
    });
    Ok(SddLockGuard {
        path: path.to_path_buf(),
        _handle: handle,
    })
}

fn reentrant_guard(path: &Path) -> Option<SddLockGuard> {
    HELD_LOCKS.with(|locks| {
        let mut locks = locks.borrow_mut();
        let handle = locks.get(path).and_then(Weak::upgrade);
        if handle.is_none() {
            locks.remove(path);
        }
        handle.map(|handle| SddLockGuard {
            path: path.to_path_buf(),
            _handle: handle,
        })
    })
}

fn write_lock_data(file: &mut File, owner: &LockData) -> Result<(), std::io::Error> {
    let content = serde_json::to_string(owner)
        .map_err(|error| std::io::Error::other(format!("序列化锁信息失败：{error}")))?;
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(format!("{content}\n").as_bytes())?;
    file.sync_all()
}

fn holder_message(path: &Path) -> String {
    match read_lock_data(path) {
        Some(holder) => format!("命令 {} 正在运行（pid {}）", holder.command, holder.pid),
        None => "其他命令正在运行".to_string(),
    }
}

fn read_lock_data(path: &Path) -> Option<LockData> {
    if fs::symlink_metadata(path).ok()?.file_type().is_symlink() {
        return None;
    }
    let file = File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_LOCK_METADATA_BYTES {
        return None;
    }
    let mut raw = String::new();
    file.take(MAX_LOCK_METADATA_BYTES + 1)
        .read_to_string(&mut raw)
        .ok()?;
    if u64::try_from(raw.len()).ok()? > MAX_LOCK_METADATA_BYTES {
        return None;
    }
    serde_json::from_str(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_lock_metadata_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock");
        fs::write(&path, vec![b'x'; MAX_LOCK_METADATA_BYTES as usize + 1]).unwrap();

        assert!(read_lock_data(&path).is_none());
    }
}
