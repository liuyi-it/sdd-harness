//! 知识图谱路由：CodeGraph 查询 + 降级链。
//!
//! 路由策略：
//! - 所有 intent 统一交给 CodeGraph。
//! - CodeGraph 不可用或失败时使用受限文件扫描（degraded=true）。

use serde_json::json;

use crate::error::SddError;

use super::codegraph::CodeGraphProvider;
use super::fallback_scan::fallback_scan;
use super::provider::{KnowledgeIntent, KnowledgeProvider, QueryResult};

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

    /// 状态诊断（只探测，不索引）。
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

    fn write_index_artifacts(
        &self,
        root: &str,
        diags: &[serde_json::Value],
    ) -> Result<(), SddError> {
        let fallback = fallback_scan(root, KnowledgeIntent::Architecture, "");
        let summary = ["codebaseSummary", "packageStructure", "architecture"]
            .iter()
            .filter_map(|key| fallback.payload.get(*key).and_then(|value| value.as_str()))
            .collect::<Vec<_>>()
            .join("\n\n");
        crate::state::runtime_store::write_index(root, json!(diags), summary)
    }

    /// 按 intent 统一使用 CodeGraph；失败时降级受限文件扫描。
    pub fn query(&self, root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult {
        if self.codegraph.probe().available {
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
}
