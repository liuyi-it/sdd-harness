//! 项目原生规格模型。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpecScenario {
    pub id: String,
    pub title: String,
    pub given: Vec<String>,
    pub when: Vec<String>,
    pub then: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpecRequirement {
    pub id: String,
    pub title: String,
    pub statement: String,
    pub scenarios: Vec<SpecScenario>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpecDocument {
    pub requirements: Vec<SpecRequirement>,
}

/// 规格校验失败
#[derive(Debug, Clone, PartialEq)]
pub struct SpecValidationFailure {
    pub code: String,
    pub path: String,
    pub message: String,
}
