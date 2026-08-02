//! Agent Task Protocol：Agent 行动要求、结果与约束结构。

pub mod validate;

pub use validate::{validate_task_result, TaskExecutionResult};
