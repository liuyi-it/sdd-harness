//! 状态层：工作流状态存储、文件锁。

pub mod artifact_store;
pub mod checksum;
pub mod file_lock;
pub(crate) mod paths;
pub mod runtime_store;
pub mod state_store;

pub use file_lock::{lock_sdd, SddLockGuard};
pub use runtime_store::{RuntimeDocument, RuntimeStore};
pub use state_store::{StateStore, WorkflowState};
