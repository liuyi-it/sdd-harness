//! 知识图谱路由：双引擎按 intent 路由 + 降级链。
//!
//! 路由策略（用户已确认）：
//! - impact / context / related-files / tests / routes / architecture → GitNexus 优先
//! - explore / callers / callees → CodeGraph 优先
//! - 主路由不可用或失败 → 次路由 → 受限文件扫描（degraded=true 显式暴露）

use serde_json::json;

use crate::error::SddError;

use super::codegraph::CodeGraphProvider;
use super::fallback_scan::fallback_scan;
use super::gitnexus::GitNexusProvider;
use super::provider::{KnowledgeIntent, KnowledgeProvider, QueryResult};

pub struct KnowledgeRouter {
    pub gitnexus: GitNexusProvider,
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
            gitnexus: GitNexusProvider::default(),
            codegraph: CodeGraphProvider::default(),
        }
    }

    /// 初始化：对 PATH 中可用的引擎执行索引，写入 .sdd/index/knowledge.json 诊断。
    /// 引擎不可用时只记录诊断，不阻断初始化。
    /// `timeout_ms` 控制单次索引超时（init 用短超时避免阻塞初始化）。
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

    /// 引擎索引诊断（不写盘，供 codebase status/doctor 使用）
    pub fn index_diagnostics(
        &self,
        root: &str,
        timeout_ms: u64,
        rebuild: bool,
    ) -> Vec<serde_json::Value> {
        let providers: [&dyn KnowledgeProvider; 2] = [&self.gitnexus, &self.codegraph];
        providers
            .iter()
            .map(|p| {
                let probe = p.probe();
                let index = if probe.available {
                    if rebuild {
                        p.rebuild(root, timeout_ms)
                    } else {
                        p.index(root, timeout_ms)
                    }
                } else {
                    super::provider::IndexResult {
                        ok: false,
                        degraded: true,
                        reason: probe.message.clone(),
                    }
                };
                json!({
                    "provider": p.name(),
                    "installed": probe.available,
                    "version": probe.version,
                    "indexed": index.ok,
                    "degraded": index.degraded,
                    "reason": index.reason,
                })
            })
            .collect()
    }

    /// 状态诊断（只探测，不索引）
    pub fn status(&self, root: &str) -> Vec<serde_json::Value> {
        let providers: [&dyn KnowledgeProvider; 2] = [&self.gitnexus, &self.codegraph];
        providers
            .iter()
            .map(|p| {
                let probe = p.probe();
                json!({
                    "provider": p.name(),
                    "installed": probe.available,
                    "indexed": probe.available && p.indexed(root),
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
        let dir = std::path::Path::new(root).join(".sdd/index");
        std::fs::create_dir_all(&dir).map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("创建索引诊断目录失败：{e}"))
        })?;
        let content = serde_json::to_string_pretty(diags)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化索引诊断失败：{e}")))?;
        std::fs::write(dir.join("knowledge.json"), content)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入索引诊断失败：{e}")))?;

        let fallback = fallback_scan(root, KnowledgeIntent::Architecture, "");
        let summary = ["codebaseSummary", "packageStructure", "architecture"]
            .iter()
            .filter_map(|key| fallback.payload.get(*key).and_then(|value| value.as_str()))
            .collect::<Vec<_>>()
            .join("\n\n");
        std::fs::write(dir.join("summary.md"), summary).map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("写入 summary.md 失败：{e}"))
        })?;
        Ok(())
    }

    /// 按 intent 路由查询；两级引擎都不可用或失败时降级受限文件扫描
    pub fn query(&self, root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult {
        let primary: &dyn KnowledgeProvider = match intent {
            KnowledgeIntent::Explore | KnowledgeIntent::Callers | KnowledgeIntent::Callees => {
                &self.codegraph
            }
            _ => &self.gitnexus,
        };
        let secondary: &dyn KnowledgeProvider = if std::ptr::eq(primary, &self.gitnexus) {
            &self.codegraph
        } else {
            &self.gitnexus
        };

        if primary.probe().available {
            let result = primary.query(root, intent, query);
            if !result.degraded {
                return result;
            }
        }
        if secondary.probe().available {
            let result = secondary.query(root, intent, query);
            if !result.degraded {
                return result;
            }
        }
        fallback_scan(root, intent, query)
    }

    /// 直接执行受限文件扫描（供 codebase query 显式降级）
    pub fn fallback_scan(root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult {
        fallback_scan(root, intent, query)
    }
}
