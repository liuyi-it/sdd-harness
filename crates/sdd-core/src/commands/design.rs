//! design 命令：生成供 plan.md 使用的技术方案并写入机器状态。
//!
//! 机器设计字段位于 `.sdd/runtime.json`，change 目录只保留 design.md。

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::contracts::CommandResult;
use crate::engines::tdd::tdd_engine::{DesignInput, TddEngine};
use crate::error::SddError;
use crate::state::file_lock::lock_sdd;
use crate::state::StateStore;

pub fn run_design(
    cwd: &str,
    args: Option<&serde_json::Value>,
    engine: &TddEngine,
) -> Result<CommandResult, SddError> {
    let timeout_ms = args
        .and_then(|a| a.get("timeout"))
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0) as u64);
    let _guard = lock_sdd(cwd, "sdd design", None, timeout_ms)?;

    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    let change_id = current_change_id(&state)?;
    let change_dir = PathBuf::from(cwd).join(".sdd/changes").join(&change_id);
    crate::state::artifact_store::verify_artifact(cwd, &format!("{change_id}:spec"))?;

    let mut spec_json = crate::state::runtime_store::read_change_field(cwd, &change_id, "spec")?
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 spec"))?;
    let spec = fs::read_to_string(change_dir.join("spec.md"))
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 spec.md 失败：{e}")))?;
    let spec_digest = crate::policies::digest::digest(&spec);
    let impact = spec_json
        .get("impact")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let codebase_summary = crate::state::runtime_store::read_index_field(cwd, "summary")?
        .and_then(|value| value.as_str().map(String::from))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少索引摘要"))?;
    // 不可信上下文边界（与 build.rs 的 Context Pack 做法一致）：代码库摘要是外部输入，
    // 先转义 END 标记防止注入逃逸，再按字符截断 8192，最后用 BEGIN/END 标记包裹。
    let safe_summary = codebase_summary
        .replace(
            "END_UNTRUSTED_CODEBASE_CONTEXT",
            "ESCAPED_END_UNTRUSTED_CODEBASE_CONTEXT",
        )
        .chars()
        .take(8_192)
        .collect::<String>();
    let wrapped_summary =
        format!("BEGIN_UNTRUSTED_CODEBASE_CONTEXT\n{safe_summary}\nEND_UNTRUSTED_CODEBASE_CONTEXT");
    let existing_design = spec_json
        .get("design")
        .and_then(|value| value.as_str())
        .map(String::from);
    // DesignInput 结构不动：三个代码库字段统一传包裹后的安全串，设计提示不再直接拼接原始摘要。
    let design = engine.generate_design(&DesignInput {
        spec,
        impact,
        codebase_summary: wrapped_summary.clone(),
        package_structure: wrapped_summary.clone(),
        architecture: wrapped_summary,
        existing_design,
    });
    spec_json
        .as_object_mut()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "runtime.json 的 spec 必须是对象"))?
        .insert("design".to_string(), json!(design));
    crate::state::runtime_store::write_change_fields(
        cwd,
        &change_id,
        [("spec", spec_json), ("design", json!(design.clone()))],
    )?;
    fs::write(change_dir.join("design.md"), &design)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 design.md 失败：{e}")))?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:design"),
        "design",
        &format!(".sdd/changes/{change_id}/design.md"),
        &design,
        json!({ "spec": spec_digest }),
    )?;

    store.update(|s| {
        s.current_phase = "DESIGN_READY".to_string();
        s.in_progress_phase = None;
        s.suggested_command = Some("sdd plan".to_string());
        s.last_command = Some("sdd design".to_string());
        s.last_error = None;
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
