//! Agent 生成计划所使用的任务协议。

mod protocol;

pub(crate) use protocol::valid_task_id;
pub use protocol::{PlannedVerification, TaskDefinition, TaskInterfaces, TaskStep};
