//! git 层：检查、快照、delta 与 worktree 隔离。

pub mod inspector;

pub use inspector::{GitDelta, GitInspector, GitSnapshot};
