//! 知识图谱适配层：CodeGraph 与受限文件扫描降级。

pub mod codegraph;
pub mod fallback_scan;
pub mod provider;
pub mod router;

pub use provider::{KnowledgeIntent, KnowledgeProvider, QueryResult};
pub use router::KnowledgeRouter;
