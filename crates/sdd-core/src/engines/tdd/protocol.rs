//! TDD 任务协议类型。

use serde::{Deserialize, Serialize};

pub(crate) fn valid_task_id(task_id: &str) -> bool {
    task_id.strip_prefix("TASK-").is_some_and(|sequence| {
        sequence.len() == 3 && sequence.bytes().all(|byte| byte.is_ascii_digit())
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskDefinition {
    pub id: String,
    pub title: String,
    pub execution_mode: String,
    pub requirements: Vec<String>,
    pub scenarios: Vec<String>,
    pub depends_on: Vec<String>,
    pub allowed_files: Vec<String>,
    pub expected_new_files: Vec<String>,
    pub forbidden_files: Vec<String>,
    pub interfaces: TaskInterfaces,
    pub steps: Vec<TaskStep>,
    pub verification: Vec<PlannedVerification>,
    pub done_criteria: Vec<String>,
    pub user_visible_outcome: String,
    pub acceptance_criteria: Vec<String>,
    pub test_seam: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskInterfaces {
    pub consumes: Vec<String>,
    pub produces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskStep {
    pub kind: String,
    pub instruction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlannedVerification {
    pub command: String,
    pub args: Vec<String>,
    pub expected: String,
}
