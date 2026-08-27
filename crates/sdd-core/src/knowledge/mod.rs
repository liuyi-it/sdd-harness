//! 知识图谱适配层：CodeGraph 与受限文件扫描降级。

pub mod codegraph;
pub mod fallback_scan;
pub mod provider;
pub mod router;

pub use provider::{KnowledgeIntent, QueryResult};
pub use router::KnowledgeRouter;

pub(crate) const CODEGRAPH_SUMMARY_PREFIX: &str =
    "<!-- summary-provider: codegraph degraded=false -->\n";
pub(crate) const FALLBACK_SUMMARY_PREFIX: &str =
    "<!-- summary-provider: fallback-file-scan degraded=true -->\n";
