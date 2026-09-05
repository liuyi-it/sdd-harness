//! codebase 命令：代码库上下文管理（status/doctor/index/query/rebuild）。
//!
//! 底层由 knowledge 模块（CodeGraph）提供能力。

use crate::contracts::{CliWarning, CommandResult};
use crate::error::SddError;
use crate::knowledge::provider::KnowledgeIntent;
use crate::knowledge::router::{KnowledgeIndex, KnowledgeRouter};
use crate::state::artifact_store::ArtifactRecord;
use crate::state::state_store::{
    WorkflowState, INDEX_STATUS_INDEX_READY, INDEX_STATUS_UNAVAILABLE,
};

const SUB_COMMANDS: [&str; 5] = ["status", "doctor", "index", "query", "rebuild"];

pub fn run_codebase(
    cwd: &str,
    args: Option<&serde_json::Value>,
) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "sub", "query", "intent"])?;
    let empty = serde_json::Value::Null;
    let args = args.unwrap_or(&empty);
    let sub = super::string_arg(Some(args), "sub")?
        .filter(|s| SUB_COMMANDS.contains(s))
        .ok_or_else(|| {
            SddError::new(
                "E_INVALID_PHASE_COMMAND",
                "codebase 需要子命令（status/doctor/index/query/rebuild）",
            )
        })?;
    if sub != "query" && (args.get("query").is_some() || args.get("intent").is_some()) {
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            "query 与 intent 只适用于 codebase query",
        ));
    }

    let router = KnowledgeRouter::new();
    let timeout_ms = super::timeout_ms(Some(args))?.unwrap_or(match sub {
        "status" | "doctor" => 15_000,
        "query" => 60_000,
        _ => 600_000,
    });
    let writes_index = sub == "index" || sub == "rebuild";
    let _guard = if writes_index {
        Some(crate::state::file_lock::lock_initialized_sdd(
            cwd,
            Some(timeout_ms),
        )?)
    } else {
        None
    };
    if writes_index {
        let state = crate::state::RuntimeStore::new(cwd.to_string())
            .read()?
            .state;
        if !state.initialized {
            return Err(
                SddError::new("E_NOT_INITIALIZED", "请先运行 sdd init 再持久化代码库索引")
                    .with_next("sdd init"),
            );
        }
    }
    let mut persisted_state = None;
    let (result, degraded): (serde_json::Value, bool) = match sub {
        "status" => {
            let providers = router.status(cwd, timeout_ms);
            let degraded = providers.iter().any(|provider| provider.degraded);
            (serde_json::json!({ "providers": providers }), degraded)
        }
        "doctor" => {
            let providers = router.status(cwd, timeout_ms);
            let degraded = providers.iter().any(|provider| provider.degraded);
            (
                serde_json::json!({
                    "providers": providers,
                    "note": "探测 PATH 中的 codegraph 命令；不可用时降级受限文件扫描",
                }),
                degraded,
            )
        }
        "index" => {
            let index = router.initialize(cwd, timeout_ms);
            let degraded = index.diagnostics.iter().any(|provider| provider.degraded);
            let (_, document) =
                crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
                    apply_index(cwd, document, &index, |state| {
                        state.last_command = Some("sdd codebase index".to_string());
                    })
                })?;
            persisted_state = Some(document.state.current_phase);
            (
                serde_json::json!({ "providers": index.diagnostics }),
                degraded,
            )
        }
        "rebuild" => {
            let index = router.rebuild(cwd, timeout_ms);
            let degraded = index.diagnostics.iter().any(|provider| provider.degraded);
            let (_, document) =
                crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
                    apply_index(cwd, document, &index, |state| {
                        state.last_command = Some("sdd codebase rebuild".to_string());
                    })
                })?;
            persisted_state = Some(document.state.current_phase);
            (
                serde_json::json!({ "providers": index.diagnostics }),
                degraded,
            )
        }
        "query" => {
            let query = super::string_arg(Some(args), "query")?
                .filter(|query| !query.trim().is_empty())
                .ok_or_else(|| {
                    SddError::new("E_INVALID_PHASE_COMMAND", "codebase query 需要非空查询词")
                })?;
            let intent_name = super::string_arg(Some(args), "intent")?.unwrap_or("impact");
            let intent = KnowledgeIntent::parse(intent_name).ok_or_else(|| {
                SddError::new(
                    "E_INVALID_PHASE_COMMAND",
                    &format!("未知 codebase intent：{intent_name}"),
                )
            })?;
            let query_result = router.query(cwd, intent, query, timeout_ms);
            let degraded = query_result.degraded;
            (
                serde_json::json!({
                    "provider": query_result.provider,
                    "degraded": degraded,
                    "confidence": query_result.confidence,
                    "reason": query_result.reason,
                    "intent": intent.as_str(),
                    "payload": query_result.payload,
                }),
                degraded,
            )
        }
        _ => unreachable!("sub 已在入口过滤"),
    };
    let state = match persisted_state {
        Some(state) => state,
        None => crate::commands::status::read_phase(cwd)?,
    };

    Ok(CommandResult {
        ok: true,
        state,
        exit_code: 0,
        change_id: None,
        next: None,
        data: Some(result),
        rendered: None,
        warnings: degraded.then(|| {
            vec![CliWarning::new(
                "W_KNOWLEDGE_UNAVAILABLE",
                "CodeGraph 未提供可用索引，已降级为受限文件扫描",
            )
            .with_next("sdd codebase doctor")]
        }),
        action_required: None,
        error: None,
    })
}

pub(crate) fn apply_index<F>(
    cwd: &str,
    document: &mut crate::state::RuntimeDocument,
    index: &KnowledgeIndex,
    update: F,
) -> Result<(), SddError>
where
    F: FnOnce(&mut WorkflowState),
{
    let diagnostic = index
        .diagnostics
        .first()
        .expect("CodeGraph 必须返回一条索引诊断");
    let provider = if diagnostic.degraded {
        "fallback-file-scan"
    } else {
        diagnostic.provider
    };
    let degraded_reason = if diagnostic.degraded {
        Some(
            diagnostic
                .reason
                .clone()
                .expect("失败的 CodeGraph 索引诊断必须提供原因"),
        )
    } else {
        None
    };
    document.index = serde_json::json!({
        "diagnostics": &index.diagnostics,
        "summary": &index.summary,
        "updatedAt": crate::state::state_store::now_iso(),
    });
    crate::state::artifact_store::record_artifacts_in(
        cwd,
        document,
        vec![
            ArtifactRecord {
                key: "index:knowledge",
                artifact_type: "summary",
                content_path: "runtime://index/diagnostics",
                inputs: serde_json::json!({ "providers": ["codegraph"] }),
            },
            ArtifactRecord {
                key: "index:summary",
                artifact_type: "summary",
                content_path: "runtime://index/summary",
                inputs: serde_json::json!({ "providers": ["codegraph"] }),
            },
        ],
    )?;
    crate::state::state_store::apply_state_update(&mut document.state, |state| {
        state.index_status = if diagnostic.degraded {
            INDEX_STATUS_UNAVAILABLE.to_string()
        } else {
            INDEX_STATUS_INDEX_READY.to_string()
        };
        state.codebase_provider = provider.to_string();
        state.degraded = diagnostic.degraded;
        state.degraded_reason = degraded_reason;
        update(state);
    })
}
