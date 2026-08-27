//! design 命令：生成供 plan.md 使用的技术方案并写入机器状态。
//!
//! 机器设计字段位于 `.sdd/runtime.json`，change 目录只保留 design.md。

use std::fs;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::contracts::CommandResult;
use crate::engines::tdd::tdd_engine::{DesignInput, TddEngine};
use crate::error::SddError;
use crate::state::artifact_store::ArtifactRecord;
use crate::state::file_lock::lock_initialized_sdd;

pub fn run_design(
    cwd: &str,
    args: Option<&serde_json::Value>,
    engine: &TddEngine,
) -> Result<CommandResult, SddError> {
    let timeout_ms = super::timeout_ms(args)?;
    let _guard = lock_initialized_sdd(cwd, "sdd design", None, timeout_ms)?;

    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let state = runtime.state.clone();
    super::ensure_phase(cwd, &state, "design", args)?;
    super::validate_args(args, &["timeout", "changeId"])?;
    let change_id = current_change_id(&state)?;
    let change_dir = crate::state::paths::change_dir(cwd, &change_id, false)?;
    crate::state::artifact_store::verify_artifacts_in(
        cwd,
        &runtime,
        [format!("{change_id}:spec")],
    )?;

    let spec_json = runtime
        .changes
        .get(&change_id)
        .and_then(|change| change.get("spec"))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 spec"))?;
    let spec_path = change_dir.join("spec.md");
    crate::safe_fs::reject_symlink(&spec_path, "spec.md")?;
    let spec = fs::read_to_string(spec_path)
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 spec.md 失败：{e}")))?;
    let spec_digest = crate::policies::digest::digest(&spec);
    let impact = spec_json
        .get("impact")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "runtime.json 的 spec 缺少 impact"))?;
    let codebase_summary = runtime
        .index
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少索引摘要"))?;
    // 不可信上下文边界（与 build.rs 的 Context Pack 做法一致）：代码库摘要是外部输入，
    // 先转义 END 标记防止注入逃逸，再按字符截断 8192，最后用 BEGIN/END 标记包裹。
    let mut safe_summary = codebase_summary.replace(
        "END_UNTRUSTED_CODEBASE_CONTEXT",
        "ESCAPED_END_UNTRUSTED_CODEBASE_CONTEXT",
    );
    if let Some((boundary, _)) = safe_summary.char_indices().nth(8_192) {
        safe_summary.truncate(boundary);
    }
    let wrapped_summary =
        format!("BEGIN_UNTRUSTED_CODEBASE_CONTEXT\n{safe_summary}\nEND_UNTRUSTED_CODEBASE_CONTEXT");
    let design = engine.generate_design(&DesignInput {
        spec: &spec,
        impact,
        codebase_context: &wrapped_summary,
    });
    crate::safe_fs::atomic_write(
        &change_dir.join("design.md"),
        design.as_bytes(),
        "design.md",
    )?;
    let artifact_key = format!("{change_id}:design");
    let content_path = format!(".sdd/changes/{change_id}/design.md");
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        let change = super::change_mut(document, &change_id)?;
        change.insert("design".to_string(), json!(design));
        crate::state::artifact_store::record_artifacts_in(
            cwd,
            document,
            vec![ArtifactRecord {
                key: &artifact_key,
                artifact_type: "design",
                content_path: &content_path,
                inputs: json!({ "spec": spec_digest }),
            }],
        )?;
        crate::state::state_store::apply_state_update(&mut document.state, |state| {
            state.current_phase = "DESIGN_READY".to_string();
            state.in_progress_phase = None;
            state.clear_failure();
            state.suggested_command = Some("sdd plan".to_string());
            state.last_command = Some("sdd design".to_string());
        })?;
        Ok(())
    })?;

    Ok(CommandResult {
        ok: true,
        state: "DESIGN_READY".to_string(),
        exit_code: 0,
        change_id: Some(change_id),
        next: Some("sdd plan".to_string()),
        data: None,
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}
