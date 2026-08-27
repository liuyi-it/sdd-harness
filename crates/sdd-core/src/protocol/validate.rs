//! Agent Task Protocol 校验。
//!
//! TaskExecutionResult 是 Agent 提交任务结果的稳定结构：
//! taskId / status(completed|failed) / evidence / verification / filesChanged。
//! 先经 schema 结构校验（收敛弱校验路径），再施加数量与长度限额。

use crate::error::SddError;

/// 限额常量（长度按 Unicode 标量值计数）
const MAX_EVIDENCE_ITEMS: usize = 64;
const MAX_EVIDENCE_OUTPUT_CHARS: usize = 8192;
const MAX_COMMAND_CHARS: usize = 2048;
const MAX_MESSAGE_CHARS: usize = 2048;
const MAX_FILES_CHANGED_ITEMS: usize = 500;
const MAX_FILE_PATH_CHARS: usize = 512;
const MAX_VERIFICATION_ITEMS: usize = 32;
const MAX_VERIFICATION_ARGS_ITEMS: usize = 64;
const MAX_VERIFICATION_ARG_CHARS: usize = 512;
const MAX_VERIFICATION_OUTPUT_CHARS: usize = 8192;
const MAX_TASK_ID_CHARS: usize = 128;

#[derive(Debug, Clone, PartialEq)]
pub struct TaskResultEvidence {
    pub command: String,
    pub passed: Option<bool>,
    pub expected_failure: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskResultVerification {
    pub command: String,
    pub args: Vec<String>,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskExecutionResult {
    pub task_id: String,
    pub status: String,
    pub evidence: Vec<TaskResultEvidence>,
    pub verification: Vec<TaskResultVerification>,
    pub files_changed: Vec<String>,
}

/// 校验原始 JSON 结构；非法或超限时返回 E_TDD_EVIDENCE_REQUIRED
pub fn validate_task_result(raw: &serde_json::Value) -> Result<TaskExecutionResult, SddError> {
    // 先走 schema 结构校验，收敛弱校验路径；失败统一映射为协议错误码
    crate::schema::validate_json("task-result", raw)
        .map_err(|e| SddError::new("E_TDD_EVIDENCE_REQUIRED", &e.message))?;

    // 限额：evidence 数量与单条输出/命令长度
    let evidence_arr = raw
        .get("evidence")
        .and_then(|v| v.as_array())
        .ok_or_else(|| invalid("evidence 必须是数组"))?;
    if evidence_arr.len() > MAX_EVIDENCE_ITEMS {
        return Err(invalid(&format!(
            "evidence 数量超过上限 {MAX_EVIDENCE_ITEMS} 条"
        )));
    }
    for (index, item) in evidence_arr.iter().enumerate() {
        if let Some(command) = item.get("command").and_then(|v| v.as_str()) {
            validate_length(
                command,
                MAX_COMMAND_CHARS,
                &format!("evidence[{index}].command"),
            )?;
        }
        if let Some(output) = item.get("output").and_then(|v| v.as_str()) {
            validate_length(
                output,
                MAX_EVIDENCE_OUTPUT_CHARS,
                &format!("evidence[{index}].output"),
            )?;
        }
    }

    // 限额：message
    if let Some(message) = raw.get("message").and_then(|v| v.as_str()) {
        validate_length(message, MAX_MESSAGE_CHARS, "message")?;
    }

    // 限额：filesChanged 数量与单条长度
    if let Some(files) = raw.get("filesChanged").and_then(|v| v.as_array()) {
        if files.len() > MAX_FILES_CHANGED_ITEMS {
            return Err(invalid(&format!(
                "filesChanged 数量超过上限 {MAX_FILES_CHANGED_ITEMS} 条"
            )));
        }
        for (index, file) in files.iter().enumerate() {
            if let Some(path) = file.as_str() {
                validate_length(path, MAX_FILE_PATH_CHARS, &format!("filesChanged[{index}]"))?;
            }
        }
    }

    // 限额：verification 数量、命令、参数和输出长度。
    let verification_arr = raw.get("verification").and_then(|v| v.as_array());
    if let Some(verification) = verification_arr {
        if verification.len() > MAX_VERIFICATION_ITEMS {
            return Err(invalid(&format!(
                "verification 数量超过上限 {MAX_VERIFICATION_ITEMS} 条"
            )));
        }
        for (index, item) in verification.iter().enumerate() {
            if let Some(command) = item.get("command").and_then(|v| v.as_str()) {
                validate_length(
                    command,
                    MAX_COMMAND_CHARS,
                    &format!("verification[{index}].command"),
                )?;
            }
            if let Some(args) = item.get("args").and_then(|v| v.as_array()) {
                if args.len() > MAX_VERIFICATION_ARGS_ITEMS {
                    return Err(invalid(&format!(
                        "verification[{index}].args 数量超过上限 {MAX_VERIFICATION_ARGS_ITEMS} 条"
                    )));
                }
                for (arg_index, arg) in args.iter().enumerate() {
                    if let Some(arg) = arg.as_str() {
                        validate_length(
                            arg,
                            MAX_VERIFICATION_ARG_CHARS,
                            &format!("verification[{index}].args[{arg_index}]"),
                        )?;
                    }
                }
            }
            if let Some(output) = item.get("output").and_then(|v| v.as_str()) {
                validate_length(
                    output,
                    MAX_VERIFICATION_OUTPUT_CHARS,
                    &format!("verification[{index}].output"),
                )?;
            }
        }
    }

    let task_id = raw
        .get("taskId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| invalid("缺少 taskId（字符串）"))?;
    validate_length(task_id, MAX_TASK_ID_CHARS, "taskId")?;
    let status = raw
        .get("status")
        .and_then(|v| v.as_str())
        .filter(|s| *s == "completed" || *s == "failed")
        .ok_or_else(|| invalid("status 必须是 completed 或 failed"))?;

    let mut evidence = Vec::new();
    for item in evidence_arr {
        let command = required_string(item, "command")?;
        evidence.push(TaskResultEvidence {
            command,
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
    let verification = verification_arr
        .into_iter()
        .flat_map(|items| items.iter())
        .map(|item| {
            Ok(TaskResultVerification {
                command: required_string(item, "command")?,
                args: optional_string_array(item, "args")?,
                passed: item
                    .get("passed")
                    .and_then(|value| value.as_bool())
                    .ok_or_else(|| invalid("verification.passed 必须是布尔值"))?,
            })
        })
        .collect::<Result<Vec<_>, SddError>>()?;

    Ok(TaskExecutionResult {
        task_id: task_id.to_string(),
        status: status.to_string(),
        evidence,
        verification,
        files_changed,
    })
}

fn required_string(raw: &serde_json::Value, field: &str) -> Result<String, SddError> {
    raw.get(field)
        .and_then(|value| value.as_str())
        .map(String::from)
        .ok_or_else(|| invalid(&format!("evidence.{field} 必须是字符串")))
}

fn optional_string_array(raw: &serde_json::Value, field: &str) -> Result<Vec<String>, SddError> {
    let Some(values) = raw.get(field) else {
        return Ok(Vec::new());
    };
    values
        .as_array()
        .ok_or_else(|| invalid(&format!("{field} 必须是字符串数组")))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(String::from)
                .ok_or_else(|| invalid(&format!("{field} 必须是字符串数组")))
        })
        .collect()
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

fn validate_length(value: &str, maximum: usize, field: &str) -> Result<(), SddError> {
    if value.chars().count() > maximum {
        return Err(invalid(&format!("{field} 超过上限 {maximum} 字符")));
    }
    Ok(())
}

fn invalid(reason: &str) -> SddError {
    SddError::new("E_TDD_EVIDENCE_REQUIRED", reason)
}
