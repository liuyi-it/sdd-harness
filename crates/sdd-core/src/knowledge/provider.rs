//! 知识图谱 Provider 抽象：CodeGraph / 受限文件扫描。
//!
//! Core 不托管外部服务进程，而是通过统一的有界子进程执行器调用 CodeGraph CLI。

use std::path::PathBuf;

use serde_json::{json, Value};

/// 查询意图。
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

/// 查询结果。
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub provider: &'static str,
    pub degraded: bool,
    pub confidence: f64,
    pub reason: Option<String>,
    pub payload: Value,
}

/// 在 PATH 中查找当前平台可执行文件。
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

/// 降级查询结果的统一构造（confidence ≤ 0.45）。
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
