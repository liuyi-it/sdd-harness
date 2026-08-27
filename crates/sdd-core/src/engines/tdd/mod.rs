//! TDD 引擎：设计文档与四阶段任务计划。

pub(crate) mod planner;
mod protocol;
pub mod tdd_engine;

pub(crate) use protocol::valid_task_id;
pub use protocol::{PlanArtifacts, PlanningInput, TaskDefinition};
pub use tdd_engine::{DesignInput, TddEngine};
