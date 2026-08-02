//! GitNexus 提供方：调用 `gitnexus` CLI（analyze/impact/context/query 等）。
//!
//! GitNexus 是外部知识图谱工具（Node 编写的 CLI，经子进程调用），
//! 索引目录为业务项目根的 `.gitnexus/`。

use std::path::PathBuf;

use serde_json::json;

use super::provider::{
    degraded_result, find_on_path, run_command, IndexResult, KnowledgeIntent, KnowledgeProvider,
    ProbeResult, QueryResult,
};

const QUERY_TIMEOUT_MS: u64 = 60_000;

pub struct GitNexusProvider {
    pub bin: Option<PathBuf>,
}

impl Default for GitNexusProvider {
    fn default() -> Self {
        Self {
            bin: find_on_path("gitnexus"),
        }
    }
}

impl KnowledgeProvider for GitNexusProvider {
    fn name(&self) -> &'static str {
        "gitnexus"
    }

    fn probe(&self) -> ProbeResult {
        let Some(bin) = &self.bin else {
            return ProbeResult {
                available: false,
                version: None,
                message: Some("gitnexus 未在 PATH 中找到".to_string()),
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
                message: Some("gitnexus 命令执行失败，可能未安装或版本不兼容".to_string()),
            },
        }
    }

    fn indexed(&self, root: &str) -> bool {
        let Some(bin) = &self.bin else {
            return false;
        };
        match run_command(bin, &["status"], root, 15_000) {
            Ok(output) if output.status.success() => {
                !String::from_utf8_lossy(&output.stdout).contains("not indexed")
            }
            _ => false,
        }
    }

    fn index(&self, root: &str, timeout_ms: u64) -> IndexResult {
        let Some(bin) = &self.bin else {
            return IndexResult {
                ok: false,
                degraded: true,
                reason: Some("gitnexus 不可用".to_string()),
            };
        };
        match run_command(bin, &["analyze", "--index-only"], root, timeout_ms) {
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
            return degraded_result("gitnexus", "gitnexus 未在 PATH 中找到", intent);
        };
        let args: Vec<&str> = match intent {
            KnowledgeIntent::Impact => vec!["impact", "--summary-only", query],
            KnowledgeIntent::Context => vec!["context", query],
            _ => vec!["query", query],
        };
        match run_command(bin, &args, root, QUERY_TIMEOUT_MS) {
            Ok(out) if out.status.success() => {
                let output = String::from_utf8_lossy(&out.stdout).trim().to_string();
                QueryResult {
                    provider: "gitnexus",
                    degraded: false,
                    confidence: 0.8,
                    reason: None,
                    payload: json!({
                        "intent": intent.as_str(),
                        "provider": "gitnexus",
                        "output": output,
                    }),
                }
            }
            Ok(out) => degraded_result(
                "gitnexus",
                String::from_utf8_lossy(&out.stderr).trim(),
                intent,
            ),
            Err(e) => degraded_result("gitnexus", &e.to_string(), intent),
        }
    }

    fn rebuild(&self, root: &str, timeout_ms: u64) -> IndexResult {
        let Some(bin) = &self.bin else {
            return IndexResult {
                ok: false,
                degraded: true,
                reason: Some("gitnexus 不可用".to_string()),
            };
        };
        match run_command(
            bin,
            &["analyze", "--index-only", "--force"],
            root,
            timeout_ms,
        ) {
            Ok(output) if output.status.success() => IndexResult {
                ok: true,
                degraded: false,
                reason: None,
            },
            Ok(output) => IndexResult {
                ok: false,
                degraded: true,
                reason: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
            },
            Err(error) => IndexResult {
                ok: false,
                degraded: true,
                reason: Some(error.to_string()),
            },
        }
    }
}
