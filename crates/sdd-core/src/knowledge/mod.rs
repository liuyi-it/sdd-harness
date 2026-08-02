//! 知识图谱适配层：GitNexus + CodeGraph 双引擎（替代 codebase-memory-mcp）。

pub mod codegraph;
pub mod fallback_scan;
pub mod gitnexus;
pub mod provider;
pub mod router;

pub use provider::{KnowledgeIntent, KnowledgeProvider, QueryResult};
pub use router::KnowledgeRouter;
