//! 知识图谱 Provider 抽象：CodeGraph / 受限文件扫描。
//!
//! Rust 版不托管外部服务进程，而是通过 `std::process::Command`
//! 子进程调用 CodeGraph CLI。
//! 语义对齐 早期 Node 实现 的
//! intent 枚举与结果结构（provider/degraded/confidence/reason/payload）。

use std::path::PathBuf;
use std::process::Output;

use serde_json::{json, Value};

/// 查询意图（与 Node 版 McpQueryIntent 一致）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeIntent {
    Impact,
    Context,
    Explore,
    Callers,
    Callees,
    RelatedFiles,
    Tests,
    Routes,
    Architecture,
}

impl KnowledgeIntent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Impact => "impact",
            Self::Context => "context",
            Self::Explore => "explore",
            Self::Callers => "callers",
            Self::Callees => "callees",
            Self::RelatedFiles => "related-files",
            Self::Tests => "tests",
            Self::Routes => "routes",
            Self::Architecture => "architecture",
        }
    }

    /// 从字符串解析 intent（命名避免与 std FromStr 混淆）
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "impact" => Some(Self::Impact),
            "context" => Some(Self::Context),
            "explore" => Some(Self::Explore),
            "callers" => Some(Self::Callers),
            "callees" => Some(Self::Callees),
            "related-files" => Some(Self::RelatedFiles),
            "tests" => Some(Self::Tests),
            "routes" => Some(Self::Routes),
            "architecture" => Some(Self::Architecture),
            _ => None,
        }
    }
}

/// 探测结果
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub available: bool,
    pub version: Option<String>,
    pub message: Option<String>,
}

/// 索引结果
#[derive(Debug, Clone)]
pub struct IndexResult {
    pub ok: bool,
    pub degraded: bool,
    pub reason: Option<String>,
}

/// 查询结果（对应 Node 版 McpQueryResult 的稳定字段）
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub provider: &'static str,
    pub degraded: bool,
    pub confidence: f64,
    pub reason: Option<String>,
    pub payload: Value,
}

/// 知识图谱提供方统一接口
pub trait KnowledgeProvider {
    fn name(&self) -> &'static str;
    fn probe(&self) -> ProbeResult;
    fn indexed(&self, _root: &str) -> bool {
        false
    }
    /// 索引仓库；`timeout_ms` 控制单次索引超时（init 用短超时避免阻塞）
    fn index(&self, root: &str, timeout_ms: u64) -> IndexResult;
    fn rebuild(&self, root: &str, timeout_ms: u64) -> IndexResult {
        self.index(root, timeout_ms)
    }
    fn query(&self, root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult;
}

/// PATH 中查找可执行文件（macOS/Windows 兼容）
pub fn find_on_path(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidates = [
            dir.join(cmd),
            dir.join(format!("{cmd}.exe")),
            dir.join(format!("{cmd}.cmd")),
        ];
        for candidate in candidates {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 带超时的子进程执行（try_wait 轮询 + mpsc 管道读取，不引入异步运行时）。
///
/// 加固点（修复子进程挂起）：
/// - 子进程放入独立进程组（Unix：process_group(0)；Windows：CREATE_NEW_PROCESS_GROUP），
///   超时/错误路径整组终止，避免子进程派生的后代继续持有管道导致本函数无限阻塞；
/// - 管道读取线程经 mpsc channel 送 chunk，主循环 try_wait 轮询 + recv_timeout 收数据；
/// - 子进程退出后对 reader 做限时回收（recv_timeout 1s），超时返回已收集数据并在
///   stdout 末尾追加截断说明，绝不无限阻塞；
/// - 输出总量上限 4MB（超出继续排空但丢弃，防止内存被灌爆）。
pub fn run_command(
    bin: &std::path::Path,
    args: &[&str],
    cwd: &str,
    timeout_ms: u64,
) -> Result<Output, std::io::Error> {
    use std::process::{Command, Stdio};

    let mut command = Command::new(bin);
    command
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // 统一注入：git 子进程禁止向终端交互式索取凭据（与 git 域的全局参数约定一致）。
        .env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // 新进程组：进程组 id 即子进程 pid，便于 kill -9 -<pid> 整组终止
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NEW_PROCESS_GROUP：配合 taskkill /T /F 整树终止
        command.creation_flags(0x0000_0200);
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().expect("stdout 已配置为 piped");
    let stderr = child.stderr.take().expect("stderr 已配置为 piped");

    // 读取线程：管道 → mpsc channel；上层经 recv_timeout 限时收取，防挂起
    let (stdout_tx, stdout_rx) = std::sync::mpsc::channel();
    let (stderr_tx, stderr_rx) = std::sync::mpsc::channel();
    let stdout_reader = std::thread::spawn(move || pump_pipe(stdout, stdout_tx));
    let stderr_reader = std::thread::spawn(move || pump_pipe(stderr, stderr_tx));

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut stdout_buf: Vec<u8> = Vec::new();
    let mut stderr_buf: Vec<u8> = Vec::new();
    let exit_status;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_status = status;
                break;
            }
            Ok(None) => {}
            Err(e) => {
                kill_process_group(&mut child);
                let _ = child.wait();
                return Err(e);
            }
        }
        if std::time::Instant::now() > deadline {
            kill_process_group(&mut child);
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("命令 {} 执行超时（{}ms）", bin.display(), timeout_ms),
            ));
        }
        // 限时收数据（50ms 粒度），同时兼顾轮询与数据吞吐
        drain_pipe(
            &stdout_rx,
            &mut stdout_buf,
            std::time::Duration::from_millis(50),
        );
        drain_pipe(
            &stderr_rx,
            &mut stderr_buf,
            std::time::Duration::from_millis(50),
        );
    }

    // 子进程已退出：限时回收剩余管道数据（合计约 1s）。超时说明子进程派生了
    // 仍持有管道的后代，此时返回已收集数据并在 stdout 末尾追加说明，绝不无限阻塞。
    let stdout_done = drain_pipe(
        &stdout_rx,
        &mut stdout_buf,
        std::time::Duration::from_secs(1),
    );
    // stderr 同样限时排空（是否收满不影响返回值）
    let _ = drain_pipe(
        &stderr_rx,
        &mut stderr_buf,
        std::time::Duration::from_secs(1),
    );
    // 不 join 读取线程：若后代仍持有管道，线程可能阻塞在管道读上，直接分离防挂起
    drop(stdout_reader);
    drop(stderr_reader);
    if !stdout_done {
        stdout_buf.extend_from_slice("\n[输出截断：子进程可能派生了持有管道的后代]".as_bytes());
    }
    Ok(Output {
        status: exit_status,
        stdout: stdout_buf,
        stderr: stderr_buf,
    })
}

/// 单条输出流总量上限（字节）。
const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

/// 管道读取线程主体：chunk 送 channel；超过上限后继续排空但丢弃（防内存灌爆），
/// 并发送 `None` 哨兵标记截断；EOF 后释放 sender（channel 断开）。
fn pump_pipe(mut pipe: impl std::io::Read, tx: std::sync::mpsc::Sender<Option<Vec<u8>>>) {
    const CHUNK_SIZE: usize = 8192;
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut total = 0usize;
    let mut sent_cap_marker = false;
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => {
                let retained = n.min(MAX_OUTPUT_BYTES.saturating_sub(total));
                if retained > 0 {
                    if tx.send(Some(buffer[..retained].to_vec())).is_err() {
                        break; // 上层已放弃接收
                    }
                    total += retained;
                }
                if retained < n {
                    if !sent_cap_marker {
                        sent_cap_marker = true;
                        let _ = tx.send(None); // 上限哨兵
                    }
                    continue; // 继续排空但丢弃
                }
            }
            Err(_) => break,
        }
    }
}

/// 从管道 channel 限时收取数据；返回 true 表示已收到 EOF（读取线程结束）。
/// `None` 是 pump_pipe 在输出超限后发送的哨兵，收到后继续排空剩余数据。
fn drain_pipe(
    rx: &std::sync::mpsc::Receiver<Option<Vec<u8>>>,
    buf: &mut Vec<u8>,
    wait: std::time::Duration,
) -> bool {
    loop {
        match rx.recv_timeout(wait) {
            Ok(Some(chunk)) => append_capped(buf, &chunk),
            Ok(None) => {} // 输出超限哨兵：数据已被丢弃，继续排空
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => return false,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return true,
        }
    }
}

/// 追加数据到输出缓冲，超过上限的部分丢弃（配合 pump_pipe 双重防护）。
fn append_capped(buf: &mut Vec<u8>, chunk: &[u8]) {
    if buf.len() >= MAX_OUTPUT_BYTES {
        return;
    }
    let remaining = MAX_OUTPUT_BYTES - buf.len();
    buf.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
}

/// best-effort 终止整个进程组（Unix：kill -9 -<pid>；Windows：taskkill /T /F）。
fn kill_process_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        use std::process::Command;
        let pgid = format!("-{}", child.id());
        let _ = Command::new("kill").args(["-9", pgid.as_str()]).status();
        let _ = child.kill();
    }
    #[cfg(windows)]
    {
        use std::process::Command;
        let pid = child.id().to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", pid.as_str(), "/T", "/F"])
            .status();
        let _ = child.kill();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = child.kill();
    }
}

/// 降级查询结果的统一构造（confidence ≤ 0.45，与 Node 版 fallback 一致）
pub fn degraded_result(
    provider: &'static str,
    reason: &str,
    intent: KnowledgeIntent,
) -> QueryResult {
    QueryResult {
        provider,
        degraded: true,
        confidence: 0.3,
        reason: Some(reason.to_string()),
        payload: json!({ "intent": intent.as_str() }),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::time::Duration;

    use super::{drain_pipe, pump_pipe, MAX_OUTPUT_BYTES};

    #[test]
    fn pipe_retains_output_up_to_the_cap_boundary() {
        let mut input = vec![b'a'; MAX_OUTPUT_BYTES - 1];
        input.extend(*b"bc");
        let (tx, rx) = std::sync::mpsc::channel();

        pump_pipe(Cursor::new(input), tx);

        let mut output = Vec::new();
        assert!(drain_pipe(&rx, &mut output, Duration::from_millis(1)));
        assert_eq!(output.len(), MAX_OUTPUT_BYTES);
        assert_eq!(output[MAX_OUTPUT_BYTES - 1], b'b');
    }
}
