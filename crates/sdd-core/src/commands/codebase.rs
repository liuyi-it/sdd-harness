//! codebase 命令：代码库上下文管理（status/doctor/index/query/rebuild）。
//!
//! 翻译自 早期 Node 实现 的分发与校验语义；
//! 底层由 knowledge 模块（CodeGraph）提供能力。

use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::knowledge::provider::KnowledgeIntent;
use crate::knowledge::router::KnowledgeRouter;

const SUB_COMMANDS: [&str; 5] = ["status", "doctor", "index", "query", "rebuild"];

pub fn run_codebase(
    cwd: &str,
    args: Option<&serde_json::Value>,
) -> Result<CommandResult, SddError> {
    let args = args.cloned().unwrap_or(serde_json::Value::Null);
    let sub = args
        .get("sub")
        .and_then(|v| v.as_str())
        .filter(|s| SUB_COMMANDS.contains(s))
        .ok_or_else(|| {
            SddError::new(
                "E_INVALID_PHASE_COMMAND",
                "codebase 需要子命令（status/doctor/index/query/rebuild）",
            )
        })?;

    let router = KnowledgeRouter::new();
    let timeout_ms = args
        .get("timeout")
        .and_then(|value| value.as_f64())
        .map(|seconds| (seconds * 1000.0) as u64)
        .unwrap_or(600_000);
    let _guard = if sub == "index" || sub == "rebuild" {
        Some(crate::state::file_lock::lock_sdd(
            cwd,
            &format!("sdd codebase {sub}"),
            None,
            Some(timeout_ms),
        )?)
    } else {
        None
    };
    let result: serde_json::Value = match sub {
        "status" => serde_json::json!({ "providers": router.status(cwd) }),
        "doctor" => serde_json::json!({
            "providers": router.status(cwd),
            "note": "探测 PATH 中的 codegraph 命令；不可用时降级受限文件扫描",
        }),
        "index" => {
            let providers = router.initialize(cwd, timeout_ms)?;
            record_index_artifacts(cwd)?;
            serde_json::json!({ "providers": providers })
        }
        "rebuild" => {
            let providers = router.rebuild(cwd, timeout_ms)?;
            record_index_artifacts(cwd)?;
            serde_json::json!({ "providers": providers })
        }
        "query" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .filter(|query| !query.trim().is_empty())
                .ok_or_else(|| {
                    SddError::new("E_INVALID_PHASE_COMMAND", "codebase query 需要非空查询词")
                })?;
            let intent_name = args
                .get("intent")
                .and_then(|v| v.as_str())
                .unwrap_or("impact");
            let intent = KnowledgeIntent::parse(intent_name).ok_or_else(|| {
                SddError::new(
                    "E_INVALID_PHASE_COMMAND",
                    &format!("未知 codebase intent：{intent_name}"),
                )
            })?;
            let query_result = router.query(cwd, intent, query);
            serde_json::json!({
                "provider": query_result.provider,
                "degraded": query_result.degraded,
                "confidence": query_result.confidence,
                "reason": query_result.reason,
                "intent": intent.as_str(),
                "payload": query_result.payload,
            })
        }
        _ => unreachable!("sub 已在入口过滤"),
    };
    let degraded = result
        .get("degraded")
        .and_then(|value| value.as_bool())
        .unwrap_or_else(|| {
            result
                .get("providers")
                .and_then(|value| value.as_array())
                .map(|providers| {
                    !providers.iter().any(|provider| {
                        provider
                            .get("indexed")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        });
    let state = crate::commands::status::read_phase(cwd)?;

    Ok(CommandResult {
        ok: true,
        state,
        exit_code: 0,
        change_id: None,
        next: None,
        data: Some(result),
        rendered: None,
        warnings: degraded.then(|| {
            vec![serde_json::json!({
                "code": "W_KNOWLEDGE_UNAVAILABLE",
                "message": "CodeGraph 未提供可用索引，已降级为受限文件扫描",
                "next": "sdd codebase doctor",
            })]
        }),
        action_required: None,
        error: None,
    })
}

pub(crate) fn record_index_artifacts(cwd: &str) -> Result<(), SddError> {
    let diagnostics = crate::state::runtime_store::read_index_field(cwd, "diagnostics")?
        .unwrap_or_else(|| serde_json::json!([]));
    let diagnostics_text = serde_json::to_string_pretty(&diagnostics).map_err(|error| {
        SddError::new("E_STATE_CORRUPTED", &format!("序列化索引诊断失败：{error}"))
    })?;
    let summary = crate::state::runtime_store::read_index_field(cwd, "summary")?
        .and_then(|value| value.as_str().map(String::from))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少索引摘要"))?;
    crate::state::artifact_store::record_artifact(
        cwd,
        "index:knowledge",
        "summary",
        "runtime://index/diagnostics",
        &diagnostics_text,
        serde_json::json!({ "providers": ["codegraph"] }),
    )?;
    crate::state::artifact_store::record_artifact(
        cwd,
        "index:summary",
        "summary",
        "runtime://index/summary",
        &summary,
        serde_json::json!({ "providers": ["codegraph"] }),
    )
}
