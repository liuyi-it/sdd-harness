//! git 层：检查、工作区事实与 worktree 隔离。

pub mod inspector;
pub mod isolation;

pub use inspector::GitInspector;
pub use isolation::{GitIsolationManager, WorktreeHandle};
