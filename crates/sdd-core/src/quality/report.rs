//! 报告模型：verify/review 报告（对应 report.schema.json）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Issue {
    pub code: String,
    pub severity: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Report {
    pub kind: String, // verify | review
    pub summary: String,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_id: Option<String>,
    #[serde(default)]
    pub issues: Vec<Issue>,
}

impl Report {
    pub fn new(kind: &str, change_id: Option<String>) -> Self {
        Self {
            kind: kind.to_string(),
            summary: String::new(),
            passed: false,
            change_id,
            issues: Vec::new(),
        }
    }
}
