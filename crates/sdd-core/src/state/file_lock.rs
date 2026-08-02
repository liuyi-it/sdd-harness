//! FileLock 为所有写命令提供仓库级串行化保护。
//!
//! 翻译自 Node 版 `packages/core/src/state/file-lock.ts`：
//! - 锁文件 `.sdd/lock` 包含 JSON 元数据（pid/command/createdAt/expiresAt）
//! - 未过期或旧进程仍存活时拒绝新锁
//! - 设置了超时且超时耗尽 → E_LOCK_TIMEOUT；未设置超时 → E_CONCURRENT_RUN
//! - 过期的锁允许抢占（删除后重试）

use std::fs::{self, OpenOptions};
use std::path::PathBuf;

use crate::error::SddError;
use crate::state::state_store::{now_iso, SDD_DIR};

const LOCK_FILE: &str = "lock";
/// 锁有效期 10 分钟（与 Node 版 expiresAt 一致）
const LOCK_TTL_SECS: u64 = 10 * 60;
/// 重试间隔
const RETRY_DELAY_MS: u64 = 50;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct LockData {
    pid: u32,
    command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    change_id: Option<String>,
    created_at: String,
    expires_at: String,
}

#[derive(Debug)]
pub struct SddLockGuard {
    path: PathBuf,
}

impl Drop for SddLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
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
    let dir = PathBuf::from(cwd).join(SDD_DIR);
    fs::create_dir_all(&dir)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("创建 .sdd 目录失败：{e}")))?;
    let path = dir.join(LOCK_FILE);
    let deadline =
        timeout_ms.map(|ms| std::time::Instant::now() + std::time::Duration::from_millis(ms));

    loop {
        match try_acquire(&path, command, change_id) {
            Ok(guard) => return Ok(guard),
            Err(Conflict::Timeout) => {
                // 未设置等待时限 → 直接冲突（对齐 Node 版 E_CONCURRENT_RUN）
                if deadline.is_none() {
                    let holder = read_lock_data(&path).ok();
                    let msg = match holder {
                        Some(h) => format!("命令 {} 正在运行（pid {}）", h.command, h.pid),
                        None => "其他命令正在运行".to_string(),
                    };
                    return Err(SddError::new("E_CONCURRENT_RUN", &msg).with_next("sdd status"));
                }
                if deadline
                    .map(|d| std::time::Instant::now() < d)
                    .unwrap_or(false)
                {
                    std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
                    continue;
                }
                let holder = read_lock_data(&path).ok();
                let msg = match holder {
                    Some(h) => format!("命令 {} 持有锁超时，无法在限定时间内获取写锁", h.command),
                    None => "等待 .sdd/lock 超时，可能有其他命令正在运行".to_string(),
                };
                return Err(SddError::new("E_LOCK_TIMEOUT", &msg).with_next("sdd status"));
            }
            Err(Conflict::Stale) => {
                // 锁已过期：删除后重试
                let _ = fs::remove_file(&path);
                std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
                continue;
            }
        }
    }
}

enum Conflict {
    /// 锁被占用且未过期（或进程存活）
    Timeout,
    /// 锁已过期，可抢占
    Stale,
}

fn try_acquire(
    path: &PathBuf,
    command: &str,
    change_id: Option<&str>,
) -> Result<SddLockGuard, Conflict> {
    let data = LockData {
        pid: std::process::id(),
        command: command.to_string(),
        change_id: change_id.map(|s| s.to_string()),
        created_at: now_iso(),
        expires_at: expires_at_iso(),
    };
    let content = match serde_json::to_string(&data) {
        Ok(c) => c,
        Err(_) => return Err(Conflict::Stale),
    };
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            use std::io::Write;
            let _ = file.write_all(format!("{content}\n").as_bytes());
            Ok(SddLockGuard { path: path.clone() })
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // 读取现有锁数据判断是否过期
            match read_lock_data(path) {
                Ok(existing) if !is_expired(&existing.expires_at) => Err(Conflict::Timeout),
                // 锁已过期但旧进程仍存活：Unix 下用 kill -0 探测；探测失败视为可抢占
                Ok(existing) => {
                    if process_alive(existing.pid) {
                        Err(Conflict::Timeout)
                    } else {
                        Err(Conflict::Stale)
                    }
                }
                // 锁文件损坏（无法解析）：按过期处理
                Err(_) => Err(Conflict::Stale),
            }
        }
        Err(_) => Err(Conflict::Stale),
    }
}

fn read_lock_data(path: &PathBuf) -> Result<LockData, ()> {
    let raw = fs::read_to_string(path).map_err(|_| ())?;
    serde_json::from_str(&raw).map_err(|_| ())
}

fn is_expired(expires_at: &str) -> bool {
    parse_iso_epoch(expires_at)
        .map(|t| t <= now_epoch())
        .unwrap_or(true)
}

fn expires_at_iso() -> String {
    let now = now_epoch();
    // 直接构造 expiresAt：当前 ISO 的 epoch + TTL
    let secs = now + LOCK_TTL_SECS;
    crate::state::state_store::format_iso_epoch(secs)
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 解析 "YYYY-MM-DDTHH:MM:SSZ" 为 epoch 秒
fn parse_iso_epoch(iso: &str) -> Option<u64> {
    let iso = iso.trim_end_matches('Z');
    let mut parts = iso.split('T');
    let date = parts.next()?;
    let time = parts.next()?;
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let m: u32 = dp.next()?.parse().ok()?;
    let d: u32 = dp.next()?.parse().ok()?;
    let mut tp = time.split(':');
    let h: u32 = tp.next()?.parse().ok()?;
    let mi: u32 = tp.next()?.parse().ok()?;
    let s: u32 = tp.next()?.parse().ok()?;
    let days = days_from_civil(y, m, d);
    Some((days * 86_400 + i64::from(h) * 3600 + i64::from(mi) * 60 + i64::from(s)) as u64)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m as i64 - 3 } else { m as i64 + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// 探测进程是否存活：Unix 用 kill -0，Windows 简化为按过期时间判断
fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use std::process::Command;
        let status = Command::new("kill").args(["-0", &pid.to_string()]).status();
        match status {
            Ok(s) => s.success(),
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
