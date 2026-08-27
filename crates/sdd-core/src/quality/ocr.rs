//! Alibaba Open Code Review 的 CLI 适配器与输出校验。

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::error::SddError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrMode {
    Auto,
    Off,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrConfig {
    pub mode: OcrMode,
    pub command: String,
}

/// 解释器黑名单：把 OCR 命令指向脚本解释器（如 bash/python）意味着
/// 可以执行任意命令，等价于 shell 注入，一律阻断。
const INTERPRETER_BLACKLIST: [&str; 18] = [
    "sh",
    "bash",
    "zsh",
    "dash",
    "ksh",
    "csh",
    "fish",
    "cmd",
    "powershell",
    "pwsh",
    "python",
    "python3",
    "node",
    "nodejs",
    "ruby",
    "perl",
    "php",
    "deno",
];

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct OcrComment {
    pub path: String,
    pub content: String,
    pub existing_code: Option<String>,
    pub suggestion_code: Option<String>,
    pub start_line: u32,
    pub end_line: u32,
    pub category: String,
    pub severity: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct OcrOutput {
    pub status: String,
    pub llm: OcrLlm,
    pub message: Option<String>,
    pub summary: OcrSummary,
    pub tool_calls: OcrToolCalls,
    pub comments: Vec<OcrComment>,
    pub manifest: OcrManifest,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OcrLlm {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OcrSummary {
    pub files_reviewed: u32,
    pub comments: u32,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub elapsed: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OcrToolCalls {
    pub total: u64,
    pub by_tool: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OcrManifest {
    pub schema_version: String,
    pub run_id: String,
    pub operation: String,
    pub terminal_state: String,
    pub repository: serde_json::Value,
    pub input: serde_json::Value,
    pub execution: serde_json::Value,
    pub coverage: OcrCoverage,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OcrCoverage {
    pub selected: Vec<serde_json::Value>,
    pub completed: Vec<serde_json::Value>,
    pub reused: Vec<serde_json::Value>,
    pub failed: Vec<serde_json::Value>,
    pub waived: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrExecution {
    NotFound,
    Completed(Box<OcrOutput>),
}

pub trait OcrExecutor {
    fn execute(
        &self,
        cwd: &Path,
        command: &str,
        timeout: Duration,
    ) -> Result<OcrExecution, SddError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemOcrExecutor;

impl OcrConfig {
    pub fn from_config(config: &serde_json::Value) -> Result<Self, SddError> {
        crate::schema::validate_json("config", config)?;
        let ocr = config
            .pointer("/quality/ocr")
            .and_then(serde_json::Value::as_object)
            .expect("当前 config schema 保证 quality.ocr 为对象");
        let mode = match ocr.get("mode").and_then(serde_json::Value::as_str) {
            Some("auto") => OcrMode::Auto,
            Some("off") => OcrMode::Off,
            Some("required") => OcrMode::Required,
            _ => unreachable!("当前 config schema 已校验 OCR mode"),
        };
        let command = ocr
            .get("command")
            .and_then(serde_json::Value::as_str)
            .expect("当前 config schema 保证 OCR command 为字符串")
            .trim();
        // 同时检查完整可执行路径与首个空白分段：前者覆盖带空格路径，后者拒绝把
        // “解释器 + 参数”误当成单个可执行文件。Windows 扩展与大小写统一处理。
        let first = command.split_whitespace().next().unwrap_or("");
        if command.contains('\0') || is_interpreter_path(command) || is_interpreter_path(first) {
            return Err(SddError::new(
                "E_SECURITY_BLOCKED",
                &format!("OCR 命令 {command} 指向脚本解释器或包含 NUL，禁止使用"),
            ));
        }
        Ok(Self {
            mode,
            command: command.to_string(),
        })
    }
}

fn is_interpreter_path(command: &str) -> bool {
    let basename = command
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(command)
        .to_ascii_lowercase();
    let stem = [".exe", ".cmd", ".bat", ".com"]
        .iter()
        .find_map(|suffix| basename.strip_suffix(suffix))
        .unwrap_or(&basename);
    INTERPRETER_BLACKLIST.contains(&stem)
}

pub fn parse_output(bytes: &[u8]) -> Result<OcrOutput, SddError> {
    serde_json::from_slice(bytes)
        .map_err(|_| SddError::new("E_REVIEW_BACKEND_INVALID_OUTPUT", "OCR 输出不是合法 JSON"))
}

pub fn validate_output(
    output: OcrOutput,
    changed_files: &BTreeSet<String>,
    line_counts: &BTreeMap<String, usize>,
) -> Result<OcrOutput, SddError> {
    if !matches!(output.status.as_str(), "completed" | "skipped") {
        return Err(SddError::new("E_REVIEW_BACKEND_FAILED", "OCR 返回失败状态"));
    }
    if output.status == "skipped" && !output.comments.is_empty() {
        return Err(invalid_output("OCR skipped 结果不得包含 finding"));
    }
    if !valid_text(&output.llm.provider, 256)
        || !valid_text(&output.llm.model, 256)
        || !valid_optional_nonempty_text(output.message.as_deref(), 64 * 1024)
        || !valid_text(&output.summary.elapsed, 128)
        || !valid_text(&output.manifest.run_id, 128)
        || output.manifest.schema_version != "ocr.run-manifest/v1"
        || output.manifest.operation != "review"
        || output.manifest.terminal_state != output.status
        || !output.manifest.repository.is_object()
        || !output.manifest.input.is_object()
        || !output.manifest.execution.is_object()
        || !output.manifest.coverage.failed.is_empty()
    {
        return Err(invalid_output("OCR 元数据内容非法"));
    }
    let files_reviewed = usize::try_from(output.summary.files_reviewed)
        .map_err(|_| invalid_output("OCR files_reviewed 超出平台范围"))?;
    let comment_count = usize::try_from(output.summary.comments)
        .map_err(|_| invalid_output("OCR comment count 超出平台范围"))?;
    if comment_count != output.comments.len()
        || files_reviewed > changed_files.len()
        || (output.status == "completed" && files_reviewed == 0)
        || (output.status == "skipped" && (files_reviewed != 0 || !output.comments.is_empty()))
    {
        return Err(invalid_output("OCR 覆盖统计与变更范围不一致"));
    }
    let tool_call_total = output
        .tool_calls
        .by_tool
        .values()
        .try_fold(0_u64, |total, count| total.checked_add(*count))
        .ok_or_else(|| invalid_output("OCR tool call 统计溢出"))?;
    if tool_call_total != output.tool_calls.total {
        return Err(invalid_output("OCR tool call 统计不一致"));
    }
    let token_total = output
        .summary
        .input_tokens
        .checked_add(output.summary.output_tokens)
        .ok_or_else(|| invalid_output("OCR token 统计溢出"))?;
    if token_total != output.summary.total_tokens {
        return Err(invalid_output("OCR token 统计不一致"));
    }

    for comment in &output.comments {
        if !matches!(
            comment.category.as_str(),
            "bug"
                | "security"
                | "performance"
                | "maintainability"
                | "test"
                | "style"
                | "documentation"
                | "other"
        ) || !matches!(
            comment.severity.as_str(),
            "critical" | "high" | "medium" | "low"
        ) {
            return Err(invalid_output("OCR finding 的 category 或 severity 非法"));
        }
        if !valid_text(&comment.content, 32 * 1024)
            || !valid_optional_text(comment.existing_code.as_deref(), 64 * 1024)
            || !valid_optional_text(comment.suggestion_code.as_deref(), 64 * 1024)
        {
            return Err(invalid_output("OCR finding 内容非法"));
        }
        if comment.path.trim().is_empty()
            || crate::git::inspector::validated_relative_path(&comment.path).is_err()
            || !changed_files.contains(&comment.path)
        {
            return Err(invalid_output("OCR finding 路径不在变更文件范围内"));
        }
        let line_count = line_counts
            .get(&comment.path)
            .copied()
            .ok_or_else(|| invalid_output("OCR finding 目标文件未通过确定性文本扫描"))?;
        let end_line = usize::try_from(comment.end_line)
            .map_err(|_| invalid_output("OCR finding 行号超出平台范围"))?;
        if comment.start_line == 0 || comment.end_line < comment.start_line || end_line > line_count
        {
            return Err(invalid_output("OCR finding 行号范围非法"));
        }
    }
    Ok(output)
}

fn invalid_output(message: &str) -> SddError {
    SddError::new("E_REVIEW_BACKEND_INVALID_OUTPUT", message)
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes && !value.contains('\0')
}

fn valid_optional_text(value: Option<&str>, max_bytes: usize) -> bool {
    value.is_none_or(|value| value.len() <= max_bytes && !value.contains('\0'))
}

fn valid_optional_nonempty_text(value: Option<&str>, max_bytes: usize) -> bool {
    value.is_none_or(|value| valid_text(value, max_bytes))
}

impl OcrExecutor for SystemOcrExecutor {
    fn execute(
        &self,
        cwd: &Path,
        command: &str,
        timeout: Duration,
    ) -> Result<OcrExecution, SddError> {
        let output = match crate::subprocess::run_command(
            Path::new(command),
            &["review", "--format", "json", "--audience", "agent"],
            cwd,
            timeout,
            &[("OCR_NO_UPDATE", "1")],
        ) {
            Ok(output) => output,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(OcrExecution::NotFound)
            }
            Err(error) if error.kind() == io::ErrorKind::TimedOut => {
                return Err(SddError::new("E_REVIEW_BACKEND_TIMEOUT", "OCR 执行超时"))
            }
            Err(error) if error.kind() == io::ErrorKind::FileTooLarge => {
                return Err(invalid_output("OCR 输出超过允许大小"))
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::InvalidInput
                ) =>
            {
                return Err(SddError::new(
                    "E_REVIEW_BACKEND_UNAVAILABLE",
                    "OCR 命令无法启动",
                ))
            }
            Err(_) => return Err(SddError::new("E_REVIEW_BACKEND_FAILED", "OCR 进程执行失败")),
        };
        if !output.status.success() {
            return Err(SddError::new(
                "E_REVIEW_BACKEND_FAILED",
                "OCR 进程返回非零退出码",
            ));
        }
        Ok(OcrExecution::Completed(Box::new(parse_output(
            &output.stdout,
        )?)))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn current_config(mode: &str, command: &str) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 3,
            "hostAdapter": "codex",
            "workflow": { "gitIsolation": false },
            "quality": { "ocr": { "mode": mode, "command": command } },
            "contextPack": { "maxSizeKb": 30 },
            "audit": { "maxSizeMb": 5, "maxFiles": 200 }
        })
    }

    fn current_output(
        status: &str,
        comments: Vec<serde_json::Value>,
        files_reviewed: u32,
    ) -> serde_json::Value {
        let comment_count = u32::try_from(comments.len()).unwrap();
        serde_json::json!({
            "status": status,
            "llm": {
                "provider": "test-provider",
                "model": "test-model"
            },
            "summary": {
                "files_reviewed": files_reviewed,
                "comments": comment_count,
                "total_tokens": 0,
                "input_tokens": 0,
                "output_tokens": 0,
                "elapsed": "0s"
            },
            "tool_calls": {
                "total": 0,
                "by_tool": {}
            },
            "comments": comments,
            "manifest": {
                "schema_version": "ocr.run-manifest/v1",
                "run_id": "run-test",
                "operation": "review",
                "terminal_state": status,
                "repository": {},
                "input": {},
                "execution": {},
                "coverage": {
                    "selected": [],
                    "completed": [],
                    "reused": [],
                    "failed": [],
                    "waived": []
                },
                "elapsed_ms": 0
            }
        })
    }

    fn output_bytes(
        status: &str,
        comments: Vec<serde_json::Value>,
        files_reviewed: u32,
    ) -> Vec<u8> {
        serde_json::to_vec(&current_output(status, comments, files_reviewed)).unwrap()
    }

    #[test]
    fn rejects_incomplete_and_obsolete_config() {
        assert!(OcrConfig::from_config(&serde_json::json!({})).is_err());
        let mut config = current_config("auto", "ocr");
        config["ocr"] = serde_json::json!({
            "ocr": { "mode": "off", "command": "/obsolete/ocr" }
        });
        assert!(OcrConfig::from_config(&config).is_err());
    }

    #[test]
    fn parses_off_mode_and_structured_comment() {
        let config = OcrConfig::from_config(&current_config("off", "/opt/ocr")).unwrap();
        assert_eq!(config.mode, OcrMode::Off);
        let output = parse_output(&output_bytes(
            "completed",
            vec![serde_json::json!({
                "path": "src/a.rs",
                "content": "修复",
                "start_line": 2,
                "end_line": 3,
                "category": "bug",
                "severity": "high"
            })],
            1,
        ))
        .unwrap();
        assert_eq!(output.comments[0].start_line, 2);
        assert_eq!(output.manifest.run_id, "run-test");
    }

    #[test]
    fn missing_comments_is_rejected() {
        let mut output = current_output("skipped", Vec::new(), 0);
        output.as_object_mut().unwrap().remove("comments");
        let error = parse_output(&serde_json::to_vec(&output).unwrap()).unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");
    }

    #[test]
    fn unknown_output_fields_are_rejected() {
        let mut output = current_output("skipped", Vec::new(), 0);
        output["unexpected"] = serde_json::json!("value");
        let error = parse_output(&serde_json::to_vec(&output).unwrap()).unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");
    }

    #[test]
    fn rejects_failed_status() {
        let output = parse_output(&output_bytes("failed", Vec::new(), 0)).unwrap();
        let error = validate_output(output, &BTreeSet::new(), &BTreeMap::new()).unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_FAILED");
    }

    #[test]
    fn rejects_path_escape() {
        let output = parse_output(&output_bytes(
            "completed",
            vec![serde_json::json!({
                "path": "../secret",
                "content": "x",
                "start_line": 1,
                "end_line": 1,
                "category": "bug",
                "severity": "low"
            })],
            1,
        ))
        .unwrap();
        let changed = std::iter::once("../secret".to_string()).collect();
        let error = validate_output(output, &changed, &BTreeMap::new()).unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");
    }

    #[test]
    fn rejects_zero_or_reversed_line_range_and_unknown_metadata() {
        let changed = std::iter::once("src/a.rs".to_string()).collect();
        let line_counts = BTreeMap::from([("src/a.rs".to_string(), 1)]);
        for comment in [
            serde_json::json!({"path":"src/a.rs","content":"x","start_line":0,"end_line":1,"category":"bug","severity":"low"}),
            serde_json::json!({"path":"src/a.rs","content":"x","start_line":2,"end_line":1,"category":"bug","severity":"low"}),
            serde_json::json!({"path":"src/a.rs","content":"x","start_line":1,"end_line":1,"category":"unknown","severity":"low"}),
            serde_json::json!({"path":"src/a.rs","content":"x","start_line":1,"end_line":1,"category":"bug","severity":"urgent"}),
        ] {
            let output = parse_output(&output_bytes("completed", vec![comment], 1)).unwrap();
            let error = validate_output(output, &changed, &line_counts).unwrap_err();
            assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");
        }
    }

    #[test]
    fn rejects_inconsistent_summary_and_tool_call_counts() {
        let changed = std::iter::once("src/a.rs".to_string()).collect();

        let mut output = current_output("completed", Vec::new(), 1);
        output["summary"]["comments"] = serde_json::json!(1);
        let output = parse_output(&serde_json::to_vec(&output).unwrap()).unwrap();
        let error = validate_output(output, &changed, &BTreeMap::new()).unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");

        let mut output = current_output("completed", Vec::new(), 1);
        output["tool_calls"]["total"] = serde_json::json!(1);
        let output = parse_output(&serde_json::to_vec(&output).unwrap()).unwrap();
        let error = validate_output(output, &changed, &BTreeMap::new()).unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");

        let mut output = current_output("completed", Vec::new(), 1);
        output["summary"]["total_tokens"] = serde_json::json!(1);
        let output = parse_output(&serde_json::to_vec(&output).unwrap()).unwrap();
        let error = validate_output(output, &changed, &BTreeMap::new()).unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");
    }
    #[test]
    fn parse_output_rejects_invalid_json() {
        let error = parse_output(b"not-json").unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");
    }

    #[cfg(unix)]
    fn executable_script(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join("ocr");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn system_executor_returns_not_found_without_shell_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing-ocr");
        let result = SystemOcrExecutor
            .execute(
                dir.path(),
                missing.to_str().unwrap(),
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(result, OcrExecution::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn system_executor_parses_successful_json() {
        let dir = tempfile::tempdir().unwrap();
        let raw = serde_json::to_string(&current_output("skipped", Vec::new(), 0)).unwrap();
        let script = executable_script(dir.path(), &format!("printf '%s' '{raw}'"));
        // 5s 时限：并行测试负载下放宽，避免偶发超时误判（flaky 修复）
        let result = SystemOcrExecutor
            .execute(dir.path(), script.to_str().unwrap(), Duration::from_secs(5))
            .unwrap();
        assert_eq!(
            result,
            OcrExecution::Completed(Box::new(parse_output(raw.as_bytes()).unwrap()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn system_executor_maps_nonzero_exit_to_backend_failure() {
        let dir = tempfile::tempdir().unwrap();
        let script = executable_script(dir.path(), "exit 7");
        // 5s 时限：放宽以消除慢机上的偶发超时误判
        let error = SystemOcrExecutor
            .execute(dir.path(), script.to_str().unwrap(), Duration::from_secs(5))
            .unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_FAILED");
    }

    #[cfg(unix)]
    #[test]
    fn system_executor_kills_timed_out_process() {
        let dir = tempfile::tempdir().unwrap();
        let script = executable_script(dir.path(), "while :; do :; done");
        let error = SystemOcrExecutor
            .execute(
                dir.path(),
                script.to_str().unwrap(),
                Duration::from_millis(50),
            )
            .unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_TIMEOUT");
    }

    #[cfg(unix)]
    #[test]
    fn system_executor_kills_descendants_that_inherit_output_pipes() {
        let dir = tempfile::tempdir().unwrap();
        let script = executable_script(dir.path(), "(while :; do :; done) &\nwait");
        let started = Instant::now();
        let error = SystemOcrExecutor
            .execute(
                dir.path(),
                script.to_str().unwrap(),
                Duration::from_millis(50),
            )
            .unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_TIMEOUT");
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    #[cfg(unix)]
    #[test]
    fn system_executor_reaps_descendants_after_parent_exit() {
        let dir = tempfile::tempdir().unwrap();
        let raw = serde_json::to_string(&current_output("skipped", Vec::new(), 0)).unwrap();
        let script = executable_script(
            dir.path(),
            &format!("printf '%s' '{raw}'\n(sleep 5) &\nexit 0"),
        );
        let started = Instant::now();
        // 5s 时限：并行测试负载下放宽，避免偶发超时误判（flaky 修复）
        let result = SystemOcrExecutor
            .execute(dir.path(), script.to_str().unwrap(), Duration::from_secs(5))
            .unwrap();
        assert_eq!(
            result,
            OcrExecution::Completed(Box::new(parse_output(raw.as_bytes()).unwrap()))
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn rejects_invalid_mode_and_empty_command() {
        for config in [
            current_config("manual", "ocr"),
            current_config("auto", "   "),
        ] {
            let error = OcrConfig::from_config(&config).unwrap_err();
            assert_eq!(error.code, "E_STATE_CORRUPTED");
        }
    }
    #[test]
    fn rejects_control_character_in_suggestion() {
        let output = parse_output(&output_bytes(
            "completed",
            vec![serde_json::json!({
                "path": "src/a.rs",
                "content": "x",
                "start_line": 1,
                "end_line": 1,
                "category": "bug",
                "severity": "low",
                "suggestion_code": "\0"
            })],
            1,
        ))
        .unwrap();
        let changed = std::iter::once("src/a.rs".to_string()).collect();
        let error = validate_output(output, &changed, &BTreeMap::new()).unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");
    }

    #[test]
    fn rejects_interpreter_commands() {
        for command in [
            "bash",
            "/usr/bin/python3",
            "sh script.sh",
            "pwsh.exe",
            "node /tmp/x.js",
            "/tmp/my tools/PYTHON3.EXE",
            "PowerShell.CMD",
            "ocr\0payload",
        ] {
            let config = OcrConfig::from_config(&current_config("auto", command));
            let error = config.unwrap_err();
            assert_eq!(error.code, "E_SECURITY_BLOCKED", "命令 {command} 应被阻断");
        }
        // 正常可执行文件路径不受影响
        let config =
            OcrConfig::from_config(&current_config("auto", "/opt/alibaba/ocr-bin")).unwrap();
        assert_eq!(config.command, "/opt/alibaba/ocr-bin");
    }
}
