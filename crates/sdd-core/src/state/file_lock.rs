//! 基于操作系统独占锁的 `.sdd` 写锁。
//!
//! 锁文件保持稳定，不随 runtime 的原子替换而变化，也不写入持有者诊断文件。
//! 进程异常退出由操作系统释放锁；文件存在不代表锁仍被占用，不应删除它来抢锁。

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use crate::error::SddError;

const LOCK_FILE: &str = "lock";
const RETRY_DELAY_MS: u64 = 50;

thread_local! {
    /// 同一线程中的组合命令复用持有中的文件描述符，避免内部状态写入产生自竞争。
    /// 表中只存弱引用，不延长文件锁生命周期；其他线程和进程仍由 OS 独占锁阻断。
    static HELD_LOCKS: RefCell<HashMap<PathBuf, Weak<LockHandle>>> = RefCell::new(HashMap::new());
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

/// 获取项目写锁。
/// `timeout_ms` 为 None 时不做等待（立即冲突则报 E_CONCURRENT_RUN）。
pub fn lock_sdd(cwd: &str, timeout_ms: Option<u64>) -> Result<SddLockGuard, SddError> {
    acquire_sdd_lock(cwd, timeout_ms, true)
}

/// 获取已初始化项目的写锁；`.sdd` 不存在时直接失败且不创建任何状态目录。
pub(crate) fn lock_initialized_sdd(
    cwd: &str,
    timeout_ms: Option<u64>,
) -> Result<SddLockGuard, SddError> {
    acquire_sdd_lock(cwd, timeout_ms, false)
}

fn acquire_sdd_lock(
    cwd: &str,
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
    let path = dir.join(LOCK_FILE);
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
        match try_acquire(&path) {
            Ok(guard) => return Ok(guard),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if deadline.is_none() {
                    return Err(SddError::new(
                        "E_CONCURRENT_RUN",
                        "其他写操作正在运行，请等待其完成",
                    )
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
                    "其他写操作正在运行，无法在限定时间内获取写锁",
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

fn try_acquire(path: &Path) -> Result<SddLockGuard, std::io::Error> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock()?;

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
