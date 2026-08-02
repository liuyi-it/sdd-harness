//! codebase 命令：代码库上下文管理（status/doctor/index/query/rebuild）。
//!
//! 翻译自 Node 版 `packages/cli/src/commands/codebase.ts` 的分发与校验语义；
//! 底层由 knowledge 模块（GitNexus/CodeGraph）提供能力，替代 codebase-memory-mcp。

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
    let result: serde_json::Value = match sub {
        "status" => serde_json::json!({ "providers": router.status() }),
        "doctor" => serde_json::json!({
            "providers": router.status(),
            "note": "探测 PATH 中的 gitnexus 与 codegraph 命令；均不可用时降级受限文件扫描",
        }),
        "index" => serde_json::json!({ "providers": router.initialize(cwd) }),
        "rebuild" => serde_json::json!({ "providers": router.initialize(cwd) }),
        "query" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let intent = args
                .get("intent")
                .and_then(|v| v.as_str())
                .and_then(KnowledgeIntent::parse)
                .unwrap_or(KnowledgeIntent::Impact);
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

    Ok(CommandResult {
        ok: true,
        state: "INDEX_READY".to_string(),
        exit_code: 0,
        change_id: None,
        next: None,
        data: Some(result),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}
