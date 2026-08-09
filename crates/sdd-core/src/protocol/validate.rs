//! Agent Task Protocol 校验（翻译自 早期 Node 实现）。
//!
//! TaskExecutionResult 是 Agent 提交任务结果的稳定结构：
//! taskId / status(completed|failed) / evidence / verification / filesChanged。

use crate::error::SddError;

#[derive(Debug, Clone, PartialEq)]
pub struct TaskResultEvidence {
    pub evidence_type: String,
    pub command: String,
    pub output: String,
    pub passed: Option<bool>,
    pub expected_failure: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskExecutionResult {
    pub task_id: String,
    pub status: String,
    pub message: Option<String>,
    pub evidence: Vec<TaskResultEvidence>,
    pub files_changed: Vec<String>,
}

/// 校验原始 JSON 结构；非法时返回 E_TDD_EVIDENCE_REQUIRED
pub fn validate_task_result(raw: &serde_json::Value) -> Result<TaskExecutionResult, SddError> {
    let task_id = raw
        .get("taskId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| invalid("缺少 taskId（字符串）"))?;
    let status = raw
        .get("status")
        .and_then(|v| v.as_str())
        .filter(|s| *s == "completed" || *s == "failed")
        .ok_or_else(|| invalid("status 必须是 completed 或 failed"))?;

    let evidence_arr = raw
        .get("evidence")
        .and_then(|v| v.as_array())
        .ok_or_else(|| invalid("evidence 必须是数组"))?;
    let mut evidence = Vec::new();
    for item in evidence_arr {
        let evidence_type = required_string(item, "type")?;
        let command = required_string(item, "command")?;
        let output = required_string(item, "output")?;
        evidence.push(TaskResultEvidence {
            evidence_type,
            command,
            output,
            passed: optional_bool(item, "passed")?,
            expected_failure: optional_bool(item, "expectedFailure")?,
        });
    }
    let files_changed = raw
        .get("filesChanged")
        .and_then(|v| v.as_array())
        .ok_or_else(|| invalid("filesChanged 必须是字符串数组"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(String::from)
                .ok_or_else(|| invalid("filesChanged 必须是字符串数组"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TaskExecutionResult {
        task_id: task_id.to_string(),
        status: status.to_string(),
        message: raw
            .get("message")
            .and_then(|v| v.as_str())
            .map(String::from),
        evidence,
        files_changed,
    })
}

fn required_string(raw: &serde_json::Value, field: &str) -> Result<String, SddError> {
    raw.get(field)
        .and_then(|value| value.as_str())
        .map(String::from)
        .ok_or_else(|| invalid(&format!("evidence.{field} 必须是字符串")))
}

fn optional_bool(raw: &serde_json::Value, field: &str) -> Result<Option<bool>, SddError> {
    match raw.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| invalid(&format!("evidence.{field} 必须是布尔值"))),
    }
}

fn invalid(reason: &str) -> SddError {
    SddError::new("E_TDD_EVIDENCE_REQUIRED", reason)
}
