//! 知识图谱 Provider 抽象：GitNexus / CodeGraph / 受限文件扫描。
//!
//! 替代 Node 版的 codebase-memory-mcp 托管：Rust 版不托管 MCP server，
//! 而是通过 `std::process::Command` 子进程调用 gitnexus/codegraph CLI。
//! 语义对齐 Node 版 `packages/core/src/codebase/mcp-query.ts` 的
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
    fn index(&self, root: &str) -> IndexResult;
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

/// 带超时的子进程执行（try_wait 轮询实现超时，不引入异步运行时）
pub fn run_command(
    bin: &std::path::Path,
    args: &[&str],
    cwd: &str,
    timeout_ms: u64,
) -> Result<Output, std::io::Error> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let mut child = Command::new(bin)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_end(&mut stdout);
                }
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_end(&mut stderr);
                }
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("命令 {} 执行超时（{}ms）", bin.display(), timeout_ms),
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(e);
            }
        }
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
