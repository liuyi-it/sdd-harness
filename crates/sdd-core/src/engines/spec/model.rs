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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TechnicalDesignDecision {
    pub title: String,
    pub decision: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TechnicalDesign {
    pub summary: String,
    pub current_state: Vec<String>,
    pub decisions: Vec<TechnicalDesignDecision>,
    pub affected_files: Vec<String>,
    pub interfaces: Vec<String>,
    pub data_changes: Vec<String>,
    pub error_handling: Vec<String>,
    pub test_strategy: Vec<String>,
    pub risks: Vec<String>,
    pub rollback: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecDocument {
    pub requirements: Vec<SpecRequirement>,
    pub technical_design: TechnicalDesign,
}

/// 规格校验失败
#[derive(Debug, Clone, PartialEq)]
pub struct SpecValidationFailure {
    pub code: String,
    pub path: String,
    pub message: String,
}
