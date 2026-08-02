//! Agent Task Protocol 校验（翻译自 `packages/agent-protocol/src/validate.ts`）。
//!
//! TaskExecutionResult 是 Agent 提交任务结果的稳定结构：
//! taskId / status(completed|failed) / evidence / verification / filesChanged。

use crate::error::SddError;

#[derive(Debug, Clone, PartialEq)]
pub struct TaskResultEvidence {
    pub evidence_type: String,
    pub command: String,
    pub output: String,
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

    let mut evidence = Vec::new();
    if let Some(evidence_arr) = raw.get("evidence").and_then(|v| v.as_array()) {
        for item in evidence_arr {
            let evidence_type = item
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("note")
                .to_string();
            let command = item
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let output = item
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            evidence.push(TaskResultEvidence {
                evidence_type,
                command,
                output,
            });
        }
    }
    let files_changed: Vec<String> = raw
        .get("filesChanged")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

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

fn invalid(reason: &str) -> SddError {
    SddError::new("E_TDD_EVIDENCE_REQUIRED", reason)
}
