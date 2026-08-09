//! design 命令：生成供 plan.md 使用的技术方案并写入机器状态。
//!
//! 翻译自 早期 Node 实现：
//! 读取 spec.md，经 TddEngine 生成设计字段，状态推进 DESIGN_READY。

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
    for key in [
        format!("{change_id}:spec"),
        format!("{change_id}:spec-json"),
    ] {
        crate::state::artifact_store::verify_artifact(cwd, &key)?;
    }

    let spec_json_raw = fs::read_to_string(change_dir.join("spec.json"))
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 spec.json 失败：{e}")))?;
    let mut spec_json: serde_json::Value = serde_json::from_str(&spec_json_raw)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("spec.json 解析失败：{e}")))?;
    let spec = fs::read_to_string(change_dir.join("spec.md"))
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 spec.md 失败：{e}")))?;
    let spec_digest = crate::policies::digest::digest(&spec);
    let impact = spec_json
        .get("impact")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 从知识图谱获取代码库摘要（降级时使用文件扫描结果）
    let index_dir = PathBuf::from(cwd).join(".sdd/index");
    let codebase_summary = fs::read_to_string(index_dir.join("summary.md")).map_err(|e| {
        SddError::new(
            "E_MISSING_ARTIFACT",
            &format!("读取索引摘要 summary.md 失败：{e}"),
        )
    })?;
    let package_structure = codebase_summary.clone();
    let architecture = codebase_summary.clone();

    let existing_design = spec_json
        .get("design")
        .and_then(|value| value.as_str())
        .map(String::from);
    let design = engine.generate_design(&DesignInput {
        spec,
        impact,
        codebase_summary,
        package_structure,
        architecture,
        existing_design,
    });
    spec_json
        .as_object_mut()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "spec.json 必须是对象"))?
        .insert("design".to_string(), json!(design));
    let spec_json_text = serde_json::to_string_pretty(&spec_json)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化 spec.json 失败：{e}")))?;
    fs::write(change_dir.join("spec.json"), &spec_json_text)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("更新 spec.json 失败：{e}")))?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:spec-json"),
        "spec",
        &format!(".sdd/changes/{change_id}/spec.json"),
        &spec_json_text,
        json!({ "updatedBy": "sdd design" }),
    )?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:design"),
        "design",
        &format!(".sdd/changes/{change_id}/spec.json"),
        &spec_json_text,
        json!({
            "spec": spec_digest,
            "specJson": crate::policies::digest::digest(&spec_json_raw),
        }),
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
