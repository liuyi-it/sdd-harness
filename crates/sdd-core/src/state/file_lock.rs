//! 基于操作系统独占锁的 `.sdd` 写锁。
//!
//! 锁文件只保存当前持有者的诊断元数据；排他性由保持打开的文件描述符保证。
//! 因此进程异常退出会由操作系统自动释放锁，也不需要“过期抢占”或删除锁文件。

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::error::SddError;
use crate::state::state_store::{now_iso, SDD_DIR};

const LOCK_FILE: &str = "lock";
const RETRY_DELAY_MS: u64 = 50;

thread_local! {
    /// 同一线程中的组合命令复用持有中的文件描述符，避免内部状态写入产生自竞争。
    /// 每个线程独立记录，其他线程和进程仍由 OS 独占锁阻断。
    static HELD_LOCKS: RefCell<HashMap<PathBuf, Rc<LockHandle>>> = RefCell::new(HashMap::new());
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
    _file: File,
}

/// 持有文件描述符即持有独占锁；同一线程的嵌套调用复用句柄，最后一个 guard Drop 时释放。
#[derive(Debug)]
pub struct SddLockGuard {
    path: PathBuf,
    handle: Rc<LockHandle>,
}

impl Drop for SddLockGuard {
    fn drop(&mut self) {
        HELD_LOCKS.with(|locks| {
            let mut locks = locks.borrow_mut();
            let Some(held) = locks.get(&self.path) else {
                return;
            };
            // map 与当前 guard 各持有一个 Rc；移除 map 后由当前 guard 的 Drop 释放 File。
            if Rc::ptr_eq(held, &self.handle) && Rc::strong_count(&self.handle) == 2 {
                locks.remove(&self.path);
            }
        });
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
    lock_named(cwd, LOCK_FILE, command, change_id, timeout_ms)
}

/// auto 使用独立协调锁串行化整条 loop，同时允许内部命令继续获取 `.sdd/lock`。
pub fn lock_auto(
    cwd: &str,
    command: &str,
    change_id: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<SddLockGuard, SddError> {
    lock_named(cwd, "auto.lock", command, change_id, timeout_ms)
}

fn lock_named(
    cwd: &str,
    file_name: &str,
    command: &str,
    change_id: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<SddLockGuard, SddError> {
    let dir = PathBuf::from(cwd).join(SDD_DIR);
    fs::create_dir_all(&dir).map_err(|error| {
        SddError::new("E_STATE_CORRUPTED", &format!("创建 .sdd 目录失败：{error}"))
    })?;
    let path = dir.join(file_name);
    if let Some(guard) = reentrant_guard(&path) {
        return Ok(guard);
    }
    let deadline =
        timeout_ms.map(|ms| std::time::Instant::now() + std::time::Duration::from_millis(ms));

    loop {
        match try_acquire(&path, command, change_id) {
            Ok(guard) => return Ok(guard),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if deadline.is_none() {
                    return Err(SddError::new("E_CONCURRENT_RUN", &holder_message(&path))
                        .with_next("sdd status"));
                }
                if deadline.is_some_and(|at| std::time::Instant::now() < at) {
                    std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
                    continue;
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
        let _ = file.unlock();
        return Err(error);
    }
    let path = path.to_path_buf();
    let handle = Rc::new(LockHandle { _file: file });
    HELD_LOCKS.with(|locks| {
        locks.borrow_mut().insert(path.clone(), Rc::clone(&handle));
    });
    Ok(SddLockGuard { path, handle })
}

fn reentrant_guard(path: &Path) -> Option<SddLockGuard> {
    HELD_LOCKS.with(|locks| {
        locks
            .borrow()
            .get(path)
            .cloned()
            .map(|handle| SddLockGuard {
                path: path.to_path_buf(),
                handle,
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
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}
