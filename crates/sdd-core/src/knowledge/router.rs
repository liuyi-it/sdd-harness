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

use serde_json::json;

use crate::error::SddError;

use super::codegraph::CodeGraphProvider;
use super::fallback_scan::fallback_scan;
use super::provider::{KnowledgeIntent, KnowledgeProvider, ProbeResult, QueryResult};

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

    /// 初始化：对 PATH 中可用的 CodeGraph 执行索引，写入 runtime.json 的 index 节点。
    /// CodeGraph 不可用时只记录诊断，不阻断初始化。
    /// `timeout_ms` 控制索引超时（init 用短超时避免阻塞初始化）。
    pub fn initialize(
        &self,
        root: &str,
        timeout_ms: u64,
    ) -> Result<Vec<serde_json::Value>, SddError> {
        let diags = self.index_diagnostics(root, timeout_ms, false);
        self.write_index_artifacts(root, &diags)?;
        Ok(diags)
    }

    pub fn rebuild(&self, root: &str, timeout_ms: u64) -> Result<Vec<serde_json::Value>, SddError> {
        let diags = self.index_diagnostics(root, timeout_ms, true);
        self.write_index_artifacts(root, &diags)?;
        Ok(diags)
    }

    /// CodeGraph 索引诊断（不写盘，供 codebase status/doctor 使用）。
    pub fn index_diagnostics(
        &self,
        root: &str,
        timeout_ms: u64,
        rebuild: bool,
    ) -> Vec<serde_json::Value> {
        let providers: [&dyn KnowledgeProvider; 1] = [&self.codegraph];
        providers
            .iter()
            .map(|provider| {
                let probe = provider.probe();
                let index = if probe.available {
                    if rebuild {
                        provider.rebuild(root, timeout_ms)
                    } else {
                        provider.index(root, timeout_ms)
                    }
                } else {
                    super::provider::IndexResult {
                        ok: false,
                        degraded: true,
                        reason: probe.message.clone(),
                    }
                };
                json!({
                    "provider": provider.name(),
                    "installed": probe.available,
                    "version": probe.version,
                    "indexed": index.ok,
                    "degraded": index.degraded,
                    "reason": index.reason,
                })
            })
            .collect()
    }

    /// 状态诊断（只探测，不索引；保持即时探测）。
    pub fn status(&self, root: &str) -> Vec<serde_json::Value> {
        let providers: [&dyn KnowledgeProvider; 1] = [&self.codegraph];
        providers
            .iter()
            .map(|provider| {
                let probe = provider.probe();
                json!({
                    "provider": provider.name(),
                    "installed": probe.available,
                    "indexed": probe.available && provider.indexed(root),
                    "version": probe.version,
                    "message": probe.message,
                })
            })
            .collect()
    }

    /// 写入索引制品：summary 双轨化。
    /// CodeGraph 探测可用且索引成功时，用 `codegraph.query(root, Architecture, "")`
    /// 的非空输出作为摘要，并在首行写 meta 注释标记来源；否则沿用受限文件扫描摘要
    /// 并标记 degraded。meta 放在 summary 字符串首行，不改 runtime_store 的 write_index 签名。
    fn write_index_artifacts(
        &self,
        root: &str,
        diags: &[serde_json::Value],
    ) -> Result<(), SddError> {
        let installed = diags
            .first()
            .and_then(|d| d.get("installed"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let indexed = diags
            .first()
            .and_then(|d| d.get("indexed"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let summary = if installed && indexed {
            let result = self
                .codegraph
                .query(root, KnowledgeIntent::Architecture, "");
            let output = result
                .payload
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if !result.degraded && !output.is_empty() {
                format!("<!-- summary-provider: codegraph degraded=false -->\n{output}")
            } else {
                fallback_summary(root)
            }
        } else {
            fallback_summary(root)
        };
        crate::state::runtime_store::write_index(root, json!(diags), summary)
    }

    /// 按 intent 统一使用 CodeGraph；失败时降级受限文件扫描。
    /// query 路径对 probe 结果使用 30 秒 TTL 缓存，避免每次查询 spawn 探测命令。
    pub fn query(&self, root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult {
        let probe = self.cached_or_fresh_probe();
        if probe.available {
            let result = self.codegraph.query(root, intent, query);
            if !result.degraded {
                return result;
            }
        }
        fallback_scan(root, intent, query)
    }

    /// 直接执行受限文件扫描（供 codebase query 显式降级）。
    pub fn fallback_scan(root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult {
        fallback_scan(root, intent, query)
    }

    /// 取 probe 结果：命中 TTL 缓存直接返回，否则即时探测并回填缓存。
    /// 仅缓存 bin 已知（可执行文件可探测）的情况；bin 缺失时探测本身不 spawn 进程。
    fn cached_or_fresh_probe(&self) -> ProbeResult {
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

        let fresh = self.codegraph.probe();
        if let Some(bin) = &self.codegraph.bin {
            cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(bin.clone(), (Instant::now(), fresh.clone()));
        }
        fresh
    }
}

/// 受限文件扫描摘要（降级路径）：沿用原 codebaseSummary/packageStructure/architecture
/// 拼接，首行写 meta 注释标记降级来源。
fn fallback_summary(root: &str) -> String {
    let fallback = fallback_scan(root, KnowledgeIntent::Architecture, "");
    let body = ["codebaseSummary", "packageStructure", "architecture"]
        .iter()
        .filter_map(|key| fallback.payload.get(*key).and_then(|value| value.as_str()))
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("<!-- summary-provider: fallback-file-scan degraded=true -->\n{body}")
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
