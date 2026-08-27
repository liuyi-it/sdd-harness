//! 知识图谱路由：CodeGraph 查询 + 降级链。
//!
//! 路由策略：
//! - 所有 intent 统一交给 CodeGraph。
//! - CodeGraph 不可用或失败时使用受限文件扫描（degraded=true）。
//! - query() 对 probe 结果做进程内 TTL 缓存（30 秒），避免每次查询 spawn
//!   `codegraph --version`；initialize/status/rebuild 保持即时探测。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::codegraph::CodeGraphProvider;
use super::fallback_scan::fallback_scan;
use super::provider::{KnowledgeIntent, ProbeResult, QueryResult};

/// probe 缓存 TTL：30 秒内不重复探测。
const PROBE_TTL: Duration = Duration::from_secs(30);

/// 进程内 probe 缓存：按 bin 路径区分条目，防止不同环境/测试的探测结果互相覆盖
/// （单条目缓存会被其他 bin 的探测顶掉，导致并行场景下缓存失效）。
static PROBE_CACHE: OnceLock<Mutex<HashMap<PathBuf, (Instant, ProbeResult)>>> = OnceLock::new();

/// probe 缓存是否仍然有效（未超过 TTL）。
fn probe_is_fresh(at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(at) < PROBE_TTL
}

pub struct KnowledgeRouter {
    pub codegraph: CodeGraphProvider,
}

pub struct KnowledgeIndex {
    pub diagnostics: Vec<KnowledgeDiagnostic>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDiagnostic {
    pub provider: &'static str,
    pub installed: bool,
    pub version: Option<String>,
    pub indexed: bool,
    pub degraded: bool,
    pub reason: Option<String>,
}

impl Default for KnowledgeRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl KnowledgeRouter {
    pub fn new() -> Self {
        Self {
            codegraph: CodeGraphProvider::default(),
        }
    }

    /// 初始化：对 PATH 中可用的 CodeGraph 执行索引并返回待提交的索引快照。
    /// CodeGraph 不可用时只记录诊断，不阻断初始化。
    /// `timeout_ms` 控制索引超时（init 用短超时避免阻塞初始化）。
    pub fn initialize(&self, root: &str, timeout_ms: u64) -> KnowledgeIndex {
        self.build_index(root, timeout_ms, false)
    }

    pub fn rebuild(&self, root: &str, timeout_ms: u64) -> KnowledgeIndex {
        self.build_index(root, timeout_ms, true)
    }

    /// CodeGraph 索引诊断（不写盘，供 codebase status/doctor 使用）。
    pub fn index_diagnostics(
        &self,
        root: &str,
        timeout_ms: u64,
        rebuild: bool,
    ) -> Vec<KnowledgeDiagnostic> {
        let probe = self.codegraph.probe(Duration::from_millis(timeout_ms));
        let index = if probe.available {
            if rebuild {
                self.codegraph.rebuild(root, timeout_ms)
            } else {
                self.codegraph.index(root, timeout_ms)
            }
        } else {
            super::provider::IndexResult {
                ok: false,
                degraded: true,
                reason: probe.message.clone(),
            }
        };
        vec![KnowledgeDiagnostic {
            provider: self.codegraph.name(),
            installed: probe.available,
            version: probe.version,
            indexed: index.ok,
            degraded: index.degraded,
            reason: index.reason,
        }]
    }

    /// 状态诊断（只探测，不索引；保持即时探测）。
    pub fn status(&self, root: &str, timeout_ms: u64) -> Vec<KnowledgeDiagnostic> {
        let probe = self.codegraph.probe(Duration::from_millis(timeout_ms));
        let index_state = if probe.available {
            self.codegraph.indexed(root)
        } else {
            Ok(false)
        };
        let indexed = index_state.as_ref().copied().unwrap_or(false);
        let reason = match index_state {
            Err(reason) => Some(reason),
            Ok(true) => None,
            Ok(false) => probe
                .message
                .or_else(|| Some("CodeGraph 尚未建立索引".to_string())),
        };
        vec![KnowledgeDiagnostic {
            provider: self.codegraph.name(),
            installed: probe.available,
            version: probe.version,
            indexed,
            degraded: !indexed,
            reason,
        }]
    }

    /// 生成索引摘要：summary 双轨化。
    /// CodeGraph 探测可用且索引成功时，用 `codegraph.query(root, Architecture, "")`
    /// 的非空输出作为摘要，并在首行写 meta 注释标记来源；否则生成受限文件扫描摘要
    /// 并标记 degraded。meta 放在 summary 字符串首行，不改 runtime_store 的 write_index 签名。
    fn build_index(&self, root: &str, timeout_ms: u64, rebuild: bool) -> KnowledgeIndex {
        let mut diagnostics = self.index_diagnostics(root, timeout_ms, rebuild);
        let diagnostic = diagnostics.first().expect("CodeGraph 必须返回一条索引诊断");
        let mut summary_failure = None;
        let summary = if diagnostic.installed && diagnostic.indexed {
            let result = self.codegraph.query(
                root,
                KnowledgeIntent::Architecture,
                "",
                Duration::from_millis(timeout_ms),
            );
            let output = result.payload.get("output").and_then(|v| v.as_str());
            if !result.degraded && output.is_some_and(|output| !output.trim().is_empty()) {
                let output = output.expect("上方已验证 CodeGraph 输出存在").trim();
                format!("{}{output}", super::CODEGRAPH_SUMMARY_PREFIX)
            } else {
                let reason = result
                    .reason
                    .as_deref()
                    .unwrap_or("CodeGraph 架构查询没有返回可用输出")
                    .to_string();
                summary_failure = Some(reason.clone());
                fallback_summary(root, &reason)
            }
        } else {
            let reason = diagnostic
                .reason
                .as_deref()
                .expect("失败的 CodeGraph 索引诊断必须提供原因");
            fallback_summary(root, reason)
        };
        if let Some(reason) = summary_failure {
            let diagnostic = diagnostics
                .first_mut()
                .expect("CodeGraph 必须返回一条索引诊断");
            diagnostic.indexed = false;
            diagnostic.degraded = true;
            diagnostic.reason = Some(reason);
        }
        KnowledgeIndex {
            diagnostics,
            summary,
        }
    }

    /// 按 intent 统一使用 CodeGraph；失败时降级受限文件扫描。
    /// query 路径对 probe 结果使用 30 秒 TTL 缓存，避免每次查询 spawn 探测命令。
    pub fn query(
        &self,
        root: &str,
        intent: KnowledgeIntent,
        query: &str,
        timeout_ms: u64,
    ) -> QueryResult {
        let probe = self.cached_or_fresh_probe(timeout_ms);
        if probe.available {
            let result =
                self.codegraph
                    .query(root, intent, query, Duration::from_millis(timeout_ms));
            if !result.degraded {
                return result;
            }
            return fallback_scan(
                root,
                intent,
                query,
                result
                    .reason
                    .as_deref()
                    .expect("CodeGraph 降级查询必须提供原因"),
            );
        }
        fallback_scan(
            root,
            intent,
            query,
            probe
                .message
                .as_deref()
                .expect("不可用的 CodeGraph 探测必须提供原因"),
        )
    }

    /// 取 probe 结果：命中 TTL 缓存直接返回，否则即时探测并回填缓存。
    /// 仅缓存 bin 已知（可执行文件可探测）的情况；bin 缺失时探测本身不 spawn 进程。
    fn cached_or_fresh_probe(&self, timeout_ms: u64) -> ProbeResult {
        let cache = PROBE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(bin) = &self.codegraph.bin {
            let cached = cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(bin)
                .filter(|(at, _)| probe_is_fresh(*at, Instant::now()))
                .map(|(_, result)| result.clone());
            if let Some(result) = cached {
                return result;
            }
        }

        let fresh = self.codegraph.probe(Duration::from_millis(timeout_ms));
        if fresh.available {
            if let Some(bin) = &self.codegraph.bin {
                cache
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(bin.clone(), (Instant::now(), fresh.clone()));
            }
        }
        fresh
    }
}

/// 受限文件扫描摘要（降级路径）：拼接 codebaseSummary/packageStructure/architecture，
/// 首行写 meta 注释标记降级来源。
fn fallback_summary(root: &str, reason: &str) -> String {
    let fallback = fallback_scan(root, KnowledgeIntent::Architecture, "", reason);
    let body = ["codebaseSummary", "packageStructure", "architecture"]
        .iter()
        .filter_map(|key| fallback.payload.get(*key).and_then(|value| value.as_str()))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("{}{body}", super::FALLBACK_SUMMARY_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_cache_entry_expires_after_ttl() {
        let now = Instant::now();
        assert!(probe_is_fresh(now, now));
        // TTL 过后缓存失效
        assert!(!probe_is_fresh(now - Duration::from_secs(31), now));
    }
}
