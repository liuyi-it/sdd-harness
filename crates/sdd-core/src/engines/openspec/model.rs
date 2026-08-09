//! OpenSpec 规格模型（翻译自 早期 Node 实现）。

use serde::{Deserialize, Serialize};

pub type DeltaOperation = &'static str;

pub const DELTA_ADDED: &str = "ADDED";
pub const DELTA_MODIFIED: &str = "MODIFIED";
pub const DELTA_REMOVED: &str = "REMOVED";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecScenario {
    pub id: String,
    pub title: String,
    pub given: Vec<String>,
    pub when: Vec<String>,
    pub then: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecRequirement {
    pub id: String,
    pub title: String,
    pub statement: String,
    pub operation: String,
    pub scenarios: Vec<SpecScenario>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecDocument {
    pub title: String,
    pub requirements: Vec<SpecRequirement>,
}

/// 规格校验失败
#[derive(Debug, Clone, PartialEq)]
pub struct SpecValidationFailure {
    pub code: String,
    pub path: String,
    pub message: String,
}
