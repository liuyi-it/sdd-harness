//! CodeGraph 提供方：调用 `codegraph` CLI（init/explore/callers/callees/impact/query 等）。
//!
//! CodeGraph 是外部知识图谱工具，索引目录为业务项目根的 `.codegraph/`。
//! 子命令以 `--path <root>` 指定项目。

use std::path::PathBuf;

use serde_json::json;

use super::provider::{
    degraded_result, find_on_path, run_command, IndexResult, KnowledgeIntent, KnowledgeProvider,
    ProbeResult, QueryResult,
};

const QUERY_TIMEOUT_MS: u64 = 60_000;

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

impl KnowledgeProvider for CodeGraphProvider {
    fn name(&self) -> &'static str {
        "codegraph"
    }

    fn probe(&self) -> ProbeResult {
        let Some(bin) = &self.bin else {
            return ProbeResult {
                available: false,
                version: None,
                message: Some("codegraph 未在 PATH 中找到".to_string()),
            };
        };
        match run_command(bin, &["--version"], ".", 15_000) {
            Ok(out) if out.status.success() => ProbeResult {
                available: true,
                version: Some(String::from_utf8_lossy(&out.stdout).trim().to_string()),
                message: None,
            },
            _ => ProbeResult {
                available: false,
                version: None,
                message: Some("codegraph 命令执行失败，可能未安装或版本不兼容".to_string()),
            },
        }
    }

    fn indexed(&self, root: &str) -> bool {
        std::path::Path::new(root).join(".codegraph").exists()
    }

    fn index(&self, root: &str, timeout_ms: u64) -> IndexResult {
        let Some(bin) = &self.bin else {
            return IndexResult {
                ok: false,
                degraded: true,
                reason: Some("codegraph 不可用".to_string()),
            };
        };
        let command = if std::path::Path::new(root).join(".codegraph").exists() {
            "sync"
        } else {
            "init"
        };
        match run_command(bin, &[command], root, timeout_ms) {
            Ok(out) if out.status.success() => IndexResult {
                ok: true,
                degraded: false,
                reason: None,
            },
            Ok(out) => IndexResult {
                ok: false,
                degraded: true,
                reason: Some(String::from_utf8_lossy(&out.stderr).trim().to_string()),
            },
            Err(e) => IndexResult {
                ok: false,
                degraded: true,
                reason: Some(e.to_string()),
            },
        }
    }

    fn query(&self, root: &str, intent: KnowledgeIntent, query: &str) -> QueryResult {
        let Some(bin) = &self.bin else {
            return degraded_result("codegraph", "codegraph 未在 PATH 中找到", intent);
        };
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
        match run_command(bin, &args, root, QUERY_TIMEOUT_MS) {
            Ok(out) if out.status.success() => {
                let output = String::from_utf8_lossy(&out.stdout).trim().to_string();
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
                String::from_utf8_lossy(&out.stderr).trim(),
                intent,
            ),
            Err(e) => degraded_result("codegraph", &e.to_string(), intent),
        }
    }

    fn rebuild(&self, root: &str, timeout_ms: u64) -> IndexResult {
        let Some(bin) = &self.bin else {
            return IndexResult {
                ok: false,
                degraded: true,
                reason: Some("codegraph 不可用".to_string()),
            };
        };
        command_result(run_command(bin, &["index", "--force"], root, timeout_ms))
    }
}

fn command_result(result: Result<std::process::Output, std::io::Error>) -> IndexResult {
    match result {
        Ok(out) if out.status.success() => IndexResult {
            ok: true,
            degraded: false,
            reason: None,
        },
        Ok(out) => IndexResult {
            ok: false,
            degraded: true,
            reason: Some(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        },
        Err(error) => IndexResult {
            ok: false,
            degraded: true,
            reason: Some(error.to_string()),
        },
    }
}
