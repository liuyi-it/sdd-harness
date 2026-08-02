//! 知识图谱路由：双引擎按 intent 路由 + 降级链。
//!
//! 路由策略（用户已确认）：
//! - impact / context / related-files / tests / routes / architecture → GitNexus 优先
//! - explore / callers / callees → CodeGraph 优先
//! - 主路由不可用或失败 → 次路由 → 受限文件扫描（degraded=true 显式暴露）

use serde_json::json;

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
    pub fn initialize(&self, root: &str) -> Vec<serde_json::Value> {
        let diags = self.index_diagnostics(root);
        let _ = std::fs::create_dir_all(format!("{root}/.sdd/index"));
        if let Ok(content) = serde_json::to_string_pretty(&diags) {
            let _ = std::fs::write(format!("{root}/.sdd/index/knowledge.json"), content);
        }
        diags
    }

    /// 引擎索引诊断（不写盘，供 codebase status/doctor 使用）
    pub fn index_diagnostics(&self, root: &str) -> Vec<serde_json::Value> {
        let providers: [&dyn KnowledgeProvider; 2] = [&self.gitnexus, &self.codegraph];
        providers
            .iter()
            .map(|p| {
                let probe = p.probe();
                let index = if probe.available {
                    p.index(root)
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
                    "degraded": probe.available && index.degraded,
                    "reason": index.reason,
                })
            })
            .collect()
    }

    /// 状态诊断（只探测，不索引）
    pub fn status(&self) -> Vec<serde_json::Value> {
        let providers: [&dyn KnowledgeProvider; 2] = [&self.gitnexus, &self.codegraph];
        providers
            .iter()
            .map(|p| {
                let probe = p.probe();
                json!({
                    "provider": p.name(),
                    "installed": probe.available,
                    "version": probe.version,
                    "message": probe.message,
                })
            })
            .collect()
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
