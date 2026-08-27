//! CodeGraph 提供方：调用 `codegraph` CLI（init/explore/callers/callees/impact/query 等）。
//!
//! CodeGraph 是外部知识图谱工具，索引目录为业务项目根的 `.codegraph/`。
//! 子命令以 `--path <root>` 指定项目。

use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;

use super::provider::{
    degraded_result, find_on_path, IndexResult, KnowledgeIntent, ProbeResult, QueryResult,
};
use crate::subprocess::run_command;

pub struct CodeGraphProvider {
    pub bin: Option<PathBuf>,
}

impl Default for CodeGraphProvider {
    fn default() -> Self {
        Self {
            bin: find_on_path("codegraph"),
        }
    }
}

impl CodeGraphProvider {
    pub fn name(&self) -> &'static str {
        "codegraph"
    }

    pub fn probe(&self, timeout: Duration) -> ProbeResult {
        let Some(bin) = &self.bin else {
            return ProbeResult {
                available: false,
                version: None,
                message: Some("codegraph 未在 PATH 中找到".to_string()),
            };
        };
        match run_command(bin, &["--version"], std::path::Path::new("."), timeout, &[]) {
            Ok(out) if out.status.success() => match String::from_utf8(out.stdout) {
                Ok(version) if !version.trim().is_empty() => ProbeResult {
                    available: true,
                    version: Some(version.trim().to_string()),
                    message: None,
                },
                Ok(_) => ProbeResult {
                    available: false,
                    version: None,
                    message: Some("codegraph --version 返回空输出".to_string()),
                },
                Err(_) => ProbeResult {
                    available: false,
                    version: None,
                    message: Some("codegraph --version 返回非 UTF-8 输出".to_string()),
                },
            },
            Ok(out) => ProbeResult {
                available: false,
                version: None,
                message: Some(command_failure_reason("codegraph --version", &out)),
            },
            Err(error) => ProbeResult {
                available: false,
                version: None,
                message: Some(format!("codegraph --version 执行失败：{error}")),
            },
        }
    }

    pub fn indexed(&self, root: &str) -> Result<bool, String> {
        safe_index_directory(root)
    }

    pub fn index(&self, root: &str, timeout_ms: u64) -> IndexResult {
        let Some(bin) = &self.bin else {
            return IndexResult {
                ok: false,
                degraded: true,
                reason: Some("codegraph 不可用".to_string()),
            };
        };
        let indexed = match safe_index_directory(root) {
            Ok(indexed) => indexed,
            Err(reason) => {
                return IndexResult {
                    ok: false,
                    degraded: true,
                    reason: Some(reason),
                };
            }
        };
        let command = if indexed { "sync" } else { "init" };
        match run_command(
            bin,
            &[command],
            std::path::Path::new(root),
            Duration::from_millis(timeout_ms),
            &[],
        ) {
            Ok(out) if out.status.success() => verified_index_result(root),
            Ok(out) => IndexResult {
                ok: false,
                degraded: true,
                reason: Some(command_failure_reason(
                    &format!("codegraph {command}"),
                    &out,
                )),
            },
            Err(e) => IndexResult {
                ok: false,
                degraded: true,
                reason: Some(e.to_string()),
            },
        }
    }

    pub fn query(
        &self,
        root: &str,
        intent: KnowledgeIntent,
        query: &str,
        timeout: Duration,
    ) -> QueryResult {
        let Some(bin) = &self.bin else {
            return degraded_result("codegraph", "codegraph 未在 PATH 中找到", intent);
        };
        match safe_index_directory(root) {
            Ok(true) => {}
            Ok(false) => {
                return degraded_result(
                    "codegraph",
                    "CodeGraph 尚未建立索引，请先执行 sdd codebase index",
                    intent,
                );
            }
            Err(reason) => return degraded_result("codegraph", &reason, intent),
        }
        let path_arg = "--path";
        let args: Vec<&str> = match intent {
            KnowledgeIntent::Explore | KnowledgeIntent::Context => {
                vec!["explore", query, path_arg, root]
            }
            KnowledgeIntent::Callers => vec!["callers", query, path_arg, root],
            KnowledgeIntent::Callees => vec!["callees", query, path_arg, root],
            KnowledgeIntent::Impact => vec!["impact", query, path_arg, root],
            _ => vec!["query", query, path_arg, root],
        };
        match run_command(bin, &args, std::path::Path::new(root), timeout, &[]) {
            Ok(out) if out.status.success() => {
                let output = match String::from_utf8(out.stdout) {
                    Ok(output) if !output.trim().is_empty() => output.trim().to_string(),
                    Ok(_) => {
                        return degraded_result("codegraph", "CodeGraph 查询返回空输出", intent);
                    }
                    Err(_) => {
                        return degraded_result(
                            "codegraph",
                            "CodeGraph 查询返回非 UTF-8 输出",
                            intent,
                        );
                    }
                };
                QueryResult {
                    provider: "codegraph",
                    degraded: false,
                    confidence: 0.8,
                    reason: None,
                    payload: json!({
                        "intent": intent.as_str(),
                        "provider": "codegraph",
                        "output": output,
                    }),
                }
            }
            Ok(out) => degraded_result(
                "codegraph",
                &command_failure_reason("codegraph query", &out),
                intent,
            ),
            Err(e) => degraded_result("codegraph", &e.to_string(), intent),
        }
    }

    pub fn rebuild(&self, root: &str, timeout_ms: u64) -> IndexResult {
        let Some(bin) = &self.bin else {
            return IndexResult {
                ok: false,
                degraded: true,
                reason: Some("codegraph 不可用".to_string()),
            };
        };
        if let Err(reason) = safe_index_directory(root) {
            return IndexResult {
                ok: false,
                degraded: true,
                reason: Some(reason),
            };
        }
        match run_command(
            bin,
            &["index", "--force"],
            std::path::Path::new(root),
            Duration::from_millis(timeout_ms),
            &[],
        ) {
            Ok(out) if out.status.success() => verified_index_result(root),
            Ok(out) => IndexResult {
                ok: false,
                degraded: true,
                reason: Some(command_failure_reason("codegraph index --force", &out)),
            },
            Err(error) => IndexResult {
                ok: false,
                degraded: true,
                reason: Some(error.to_string()),
            },
        }
    }
}

fn safe_index_directory(root: &str) -> Result<bool, String> {
    let path = std::path::Path::new(root).join(".codegraph");
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "CodeGraph 索引路径 {} 是符号链接，已阻止外部读写",
            path.display()
        )),
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err(format!("CodeGraph 索引路径 {} 不是目录", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "检查 CodeGraph 索引路径 {} 失败：{error}",
            path.display()
        )),
    }
}

fn verified_index_result(root: &str) -> IndexResult {
    match safe_index_directory(root) {
        Ok(true) => IndexResult {
            ok: true,
            degraded: false,
            reason: None,
        },
        Ok(false) => IndexResult {
            ok: false,
            degraded: true,
            reason: Some("CodeGraph 命令成功但未生成 .codegraph 索引目录".to_string()),
        },
        Err(reason) => IndexResult {
            ok: false,
            degraded: true,
            reason: Some(reason),
        },
    }
}

fn command_failure_reason(command: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        format!("{command} 返回非零退出状态：{}", output.status)
    } else {
        format!("{command} 执行失败：{stderr}")
    }
}
