//! Alibaba Open Code Review 的 CLI 适配器与输出校验。

use std::collections::BTreeSet;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OcrComment {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub existing_code: Option<String>,
    #[serde(default)]
    pub suggestion_code: Option<String>,
    pub start_line: u32,
    pub end_line: u32,
    pub category: String,
    pub severity: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OcrOutput {
    pub status: String,
    #[serde(default)]
    pub comments: Vec<OcrComment>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub files_reviewed: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrExecution {
    NotFound,
    Completed(OcrOutput),
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
        let quality = config.get("quality");
        if let Some(quality) = quality {
            if !quality.is_object() {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    "runtime.json 的 quality 必须是对象",
                ));
            }
        }
        let ocr = quality
            .and_then(|value| value.get("ocr"))
            .or_else(|| config.get("ocr"));
        let Some(ocr) = ocr else {
            return Ok(Self {
                mode: OcrMode::Auto,
                command: "ocr".to_string(),
            });
        };
        let Some(ocr) = ocr.as_object() else {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "runtime.json 的 quality.ocr 必须是对象",
            ));
        };
        let mode = match ocr.get("mode").and_then(serde_json::Value::as_str) {
            None | Some("auto") => OcrMode::Auto,
            Some("off") => OcrMode::Off,
            Some("required") => OcrMode::Required,
            Some(_) => {
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    "runtime.json 的 quality.ocr.mode 必须是 auto、off 或 required",
                ))
            }
        };
        let command = ocr
            .get("command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("ocr")
            .trim();
        if command.is_empty() {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "runtime.json 的 quality.ocr.command 不能为空",
            ));
        }
        Ok(Self {
            mode,
            command: command.to_string(),
        })
    }
}

pub fn parse_output(bytes: &[u8]) -> Result<OcrOutput, SddError> {
    serde_json::from_slice(bytes)
        .map_err(|_| SddError::new("E_REVIEW_BACKEND_INVALID_OUTPUT", "OCR 输出不是合法 JSON"))
}

pub fn validate_output(
    output: OcrOutput,
    cwd: &Path,
    changed_files: &BTreeSet<String>,
) -> Result<OcrOutput, SddError> {
    if !matches!(output.status.as_str(), "success" | "skipped") {
        return Err(SddError::new("E_REVIEW_BACKEND_FAILED", "OCR 返回失败状态"));
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
        if comment.path.is_empty() || !changed_files.contains(&comment.path) {
            return Err(invalid_output("OCR finding 路径不在变更文件范围内"));
        }
        let cwd_string = cwd.to_string_lossy();
        let path = crate::git::GitInspector::resolve_repo_path(&cwd_string, &comment.path)
            .map_err(|_| invalid_output("OCR finding 路径不在仓库内"))?;
        let content = std::fs::read_to_string(&path)
            .map_err(|_| invalid_output("OCR finding 目标文件不可读"))?;
        let line_count = content.lines().count() as u32;
        if comment.start_line == 0
            || comment.end_line < comment.start_line
            || comment.end_line > line_count
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
    !value.is_empty() && value.len() <= max_bytes && !value.contains('\0')
}

fn valid_optional_text(value: Option<&str>, max_bytes: usize) -> bool {
    value.is_none_or(|value| value.len() <= max_bytes && !value.contains('\0'))
}

const MAX_STDOUT_BYTES: usize = 4 * 1024 * 1024;

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn kill_process_group(pid: u32) {
    const SIGKILL: i32 = 9;
    let process_group = -(pid as i32);
    // 进程组由子进程创建，超时时必须连同继承 stdout/stderr 的后代一起终止。
    unsafe {
        let _ = kill(process_group, SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_process_group(_pid: u32) {}

#[cfg(unix)]
extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

impl OcrExecutor for SystemOcrExecutor {
    fn execute(
        &self,
        cwd: &Path,
        command: &str,
        timeout: Duration,
    ) -> Result<OcrExecution, SddError> {
        let mut command_builder = Command::new(command);
        command_builder
            .args(["review", "--format", "json"])
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_process_group(&mut command_builder);
        let mut child = match command_builder.spawn() {
            Ok(child) => child,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(OcrExecution::NotFound)
            }
            Err(_) => {
                return Err(SddError::new(
                    "E_REVIEW_BACKEND_UNAVAILABLE",
                    "OCR 命令无法启动",
                ))
            }
        };
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SddError::new("E_REVIEW_BACKEND_FAILED", "OCR stdout 不可用"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| SddError::new("E_REVIEW_BACKEND_FAILED", "OCR stderr 不可用"))?;
        let (stdout_sender, stdout_receiver) = mpsc::sync_channel(1);
        let stdout_reader = thread::spawn(move || {
            let _ = stdout_sender.send(capture_limited(stdout, MAX_STDOUT_BYTES));
        });
        let (stderr_sender, stderr_receiver) = mpsc::sync_channel(1);
        let stderr_reader = thread::spawn(move || {
            let _ = stderr_sender.send(drain(stderr));
        });
        let deadline = Instant::now().checked_add(timeout);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                        kill_process_group(child.id());
                        let _ = child.kill();
                        let _ = child.wait();
                        drop(stdout_reader);
                        drop(stderr_reader);
                        return Err(SddError::new("E_REVIEW_BACKEND_TIMEOUT", "OCR 执行超时"));
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Err(_) => {
                    kill_process_group(child.id());
                    let _ = child.kill();
                    let _ = child.wait();
                    drop(stdout_reader);
                    drop(stderr_reader);
                    return Err(SddError::new(
                        "E_REVIEW_BACKEND_FAILED",
                        "读取 OCR 进程状态失败",
                    ));
                }
            }
        };
        // 读管道也受同一 deadline 约束；无论平台是否能杀进程树，都不能永久等待。
        let reader_timeout = deadline
            .map(|deadline| {
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_secs(1))
            })
            .unwrap_or_else(|| Duration::from_secs(1));
        kill_process_group(child.id());
        let stdout = collect_reader(stdout_receiver, stdout_reader, reader_timeout, "stdout")?;
        collect_reader(stderr_receiver, stderr_reader, reader_timeout, "stderr")?;
        if !status.success() {
            return Err(SddError::new(
                "E_REVIEW_BACKEND_FAILED",
                "OCR 进程返回非零退出码",
            ));
        }
        if stdout.truncated {
            return Err(invalid_output("OCR stdout 超过允许大小"));
        }
        Ok(OcrExecution::Completed(parse_output(&stdout.bytes)?))
    }
}

#[derive(Debug)]
struct CapturedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

fn capture_limited<R: Read>(mut reader: R, limit: usize) -> io::Result<CapturedOutput> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if bytes.len() < limit {
            let retained = (limit - bytes.len()).min(count);
            bytes.extend_from_slice(&buffer[..retained]);
            truncated |= retained < count;
        } else {
            truncated = true;
        }
    }
    Ok(CapturedOutput { bytes, truncated })
}

fn drain<R: Read>(mut reader: R) -> io::Result<()> {
    let mut buffer = [0_u8; 8192];
    while reader.read(&mut buffer)? != 0 {}
    Ok(())
}

fn collect_reader<T>(
    receiver: Receiver<io::Result<T>>,
    reader: thread::JoinHandle<()>,
    timeout: Duration,
    stream: &str,
) -> Result<T, SddError> {
    let result = receiver
        .recv_timeout(timeout)
        .map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => SddError::new(
                "E_REVIEW_BACKEND_TIMEOUT",
                &format!("读取 OCR {stream} 超时"),
            ),
            mpsc::RecvTimeoutError::Disconnected => SddError::new(
                "E_REVIEW_BACKEND_FAILED",
                &format!("读取 OCR {stream} 失败"),
            ),
        })?;
    reader.join().map_err(|_| {
        SddError::new(
            "E_REVIEW_BACKEND_FAILED",
            &format!("读取 OCR {stream} 失败"),
        )
    })?;
    result.map_err(|_| {
        SddError::new(
            "E_REVIEW_BACKEND_FAILED",
            &format!("读取 OCR {stream} 失败"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_ocr_config_defaults_to_auto_command() {
        let config = OcrConfig::from_config(&serde_json::json!({})).unwrap();
        assert_eq!(config.mode, OcrMode::Auto);
        assert_eq!(config.command, "ocr");
    }

    #[test]
    fn parses_off_mode_and_structured_comment() {
        let config = OcrConfig::from_config(&serde_json::json!({
            "quality": { "ocr": { "mode": "off", "command": "/opt/ocr" } }
        }))
        .unwrap();
        assert_eq!(config.mode, OcrMode::Off);
        let output = parse_output(
            r#"{
                "status":"success",
                "session_id":"s-1",
                "comments":[{
                    "path":"src/a.rs",
                    "content":"修复",
                    "start_line":2,
                    "end_line":3,
                    "category":"bug",
                    "severity":"high"
                }]
            }"#
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(output.comments[0].start_line, 2);
        assert_eq!(output.session_id.as_deref(), Some("s-1"));
    }

    #[test]
    fn missing_comments_defaults_to_empty_list() {
        let output = parse_output(br#"{"status":"skipped"}"#).unwrap();
        assert!(output.comments.is_empty());
    }

    #[test]
    fn rejects_failed_status() {
        let dir = tempfile::tempdir().unwrap();
        let output = parse_output(br#"{"status":"failed","comments":[]}"#).unwrap();
        let error = validate_output(output, dir.path(), &BTreeSet::new()).unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_FAILED");
    }

    #[test]
    fn rejects_path_escape() {
        let dir = tempfile::tempdir().unwrap();
        let output = parse_output(
            br#"{
                "status":"success",
                "comments":[{
                    "path":"../secret",
                    "content":"x",
                    "start_line":1,
                    "end_line":1,
                    "category":"bug",
                    "severity":"low"
                }]
            }"#,
        )
        .unwrap();
        let changed = std::iter::once("../secret".to_string()).collect();
        let error = validate_output(output, dir.path(), &changed).unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");
    }

    #[test]
    fn rejects_zero_or_reversed_line_range_and_unknown_metadata() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.rs"), "fn main() {}\n").unwrap();
        let changed = std::iter::once("src/a.rs".to_string()).collect();
        for raw in [
            br#"{"status":"success","comments":[{"path":"src/a.rs","content":"x","start_line":0,"end_line":1,"category":"bug","severity":"low"}]}"#.as_slice(),
            br#"{"status":"success","comments":[{"path":"src/a.rs","content":"x","start_line":2,"end_line":1,"category":"bug","severity":"low"}]}"#.as_slice(),
            br#"{"status":"success","comments":[{"path":"src/a.rs","content":"x","start_line":1,"end_line":1,"category":"unknown","severity":"low"}]}"#.as_slice(),
            br#"{"status":"success","comments":[{"path":"src/a.rs","content":"x","start_line":1,"end_line":1,"category":"bug","severity":"urgent"}]}"#.as_slice(),
        ] {
            let output = parse_output(raw).unwrap();
            let error = validate_output(output, dir.path(), &changed).unwrap_err();
            assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");
        }
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
        let script = executable_script(
            dir.path(),
            "printf '{\"status\":\"success\",\"comments\":[]}'",
        );
        let result = SystemOcrExecutor
            .execute(dir.path(), script.to_str().unwrap(), Duration::from_secs(1))
            .unwrap();
        assert_eq!(
            result,
            OcrExecution::Completed(OcrOutput {
                status: "success".into(),
                comments: Vec::new(),
                session_id: None,
                summary: None,
                files_reviewed: None,
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn system_executor_maps_nonzero_exit_to_backend_failure() {
        let dir = tempfile::tempdir().unwrap();
        let script = executable_script(dir.path(), "exit 7");
        let error = SystemOcrExecutor
            .execute(dir.path(), script.to_str().unwrap(), Duration::from_secs(1))
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
        let script = executable_script(
            dir.path(),
            "printf '{\"status\":\"success\",\"comments\":[]}'\n(sleep 5) &\nexit 0",
        );
        let started = Instant::now();
        let result = SystemOcrExecutor
            .execute(dir.path(), script.to_str().unwrap(), Duration::from_secs(1))
            .unwrap();
        assert_eq!(
            result,
            OcrExecution::Completed(OcrOutput {
                status: "success".into(),
                comments: Vec::new(),
                session_id: None,
                summary: None,
                files_reviewed: None,
            })
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn reader_collection_is_bounded_when_pipe_stays_open() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            let _ = sender.send(Ok(()));
        });
        let started = Instant::now();
        let error =
            collect_reader(receiver, reader, Duration::from_millis(5), "stdout").unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_TIMEOUT");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn rejects_invalid_mode_and_empty_command() {
        for config in [
            serde_json::json!({
                "quality": { "ocr": { "mode": "manual", "command": "ocr" } }
            }),
            serde_json::json!({
                "quality": { "ocr": { "mode": "auto", "command": "   " } }
            }),
        ] {
            let error = OcrConfig::from_config(&config).unwrap_err();
            assert_eq!(error.code, "E_STATE_CORRUPTED");
        }
    }
    #[test]
    fn rejects_control_character_in_suggestion() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.rs"), "fn main() {}\n").unwrap();
        let output = parse_output(
            br#"{"status":"success","comments":[{"path":"src/a.rs","content":"x","start_line":1,"end_line":1,"category":"bug","severity":"low","suggestion_code":"\u0000"}]}"#,
        )
        .unwrap();
        let changed = std::iter::once("src/a.rs".to_string()).collect();
        let error = validate_output(output, dir.path(), &changed).unwrap_err();
        assert_eq!(error.code, "E_REVIEW_BACKEND_INVALID_OUTPUT");
    }
}
