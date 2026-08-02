//! archive 命令：归档收敛为三个文件。
//!
//! 翻译自 Node 版 `packages/core/src/commands/archive.ts`：
//! - 重新验证报告摘要与文件范围
//! - 收敛 .sdd/changes/<id>/ 为 archive.json + archive.md + .archived
//! - 状态 ARCHIVED；中断后可再次执行收敛

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::commands::new::current_change_id;
use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::quality::report::Report;
use crate::state::file_lock::lock_sdd;
use crate::state::StateStore;

const ARCHIVE_MARKER: &str = ".archived";

pub fn run_archive(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    let timeout_ms = args
        .and_then(|a| a.get("timeout"))
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0) as u64);
    let _guard = lock_sdd(cwd, "sdd archive", None, timeout_ms)?;

    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;
    let change_id = current_change_id(&state)?;
    let change_dir = PathBuf::from(cwd).join(".sdd/changes").join(&change_id);

    // 已有归档标记 → 幂等收敛到 ARCHIVED
    if change_dir.join(ARCHIVE_MARKER).exists() {
        store.update(|s| {
            s.current_phase = "ARCHIVED".to_string();
            s.suggested_command = Some("sdd new <需求>".to_string());
            s.last_command = Some("sdd archive".to_string());
        })?;
        return Ok(CommandResult {
            ok: true,
            state: "ARCHIVED".to_string(),
            exit_code: 0,
            change_id: Some(change_id),
            next: Some("sdd new <需求>".to_string()),
            data: None,
            rendered: None,
            warnings: None,
            action_required: None,
            error: None,
        });
    }

    // 收集制品
    let collect = |name: &str| -> Option<String> { fs::read_to_string(change_dir.join(name)).ok() };
    let spec = collect("spec.md");
    let design = collect("design.md");
    let plan = collect("plan.md");
    let verify_report: Option<Report> =
        collect("verify-report.json").and_then(|raw| serde_json::from_str(&raw).ok());
    let review_report: Option<Report> =
        collect("review-report.json").and_then(|raw| serde_json::from_str(&raw).ok());

    // 归档内容
    let archive_md = [
        "# 归档".to_string(),
        String::new(),
        format!("## Change: {change_id}"),
        String::new(),
        "## 规格".to_string(),
        String::new(),
        spec.clone().unwrap_or_else(|| "（缺失）".to_string()),
        String::new(),
        "## 设计".to_string(),
        String::new(),
        design.clone().unwrap_or_else(|| "（缺失）".to_string()),
        String::new(),
        "## 计划".to_string(),
        String::new(),
        plan.clone().unwrap_or_else(|| "（缺失）".to_string()),
        String::new(),
        "## 验证报告".to_string(),
        String::new(),
        verify_report
            .as_ref()
            .map(|r| format!("- passed: {}\n- summary: {}", r.passed, r.summary))
            .unwrap_or_else(|| "- （缺失）".to_string()),
        String::new(),
        "## 审查报告".to_string(),
        String::new(),
        review_report
            .as_ref()
            .map(|r| format!("- passed: {}\n- summary: {}", r.passed, r.summary))
            .unwrap_or_else(|| "- （缺失）".to_string()),
    ]
    .join("\n");

    let archive_json = json!({
        "schemaVersion": "1.0.0",
        "changeId": change_id,
        "hasSpec": spec.is_some(),
        "hasDesign": design.is_some(),
        "hasPlan": plan.is_some(),
        "verifyPassed": verify_report.as_ref().map(|r| r.passed),
        "reviewPassed": review_report.as_ref().map(|r| r.passed),
        "archivedAt": crate::state::state_store::now_iso(),
    });

    fs::write(change_dir.join("archive.md"), &archive_md)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 archive.md 失败：{e}")))?;
    fs::write(
        change_dir.join("archive.json"),
        serde_json::to_string_pretty(&archive_json)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化失败：{e}")))?,
    )
    .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 archive.json 失败：{e}")))?;

    // 写组合哈希标记
    let combined = format!(
        "{}{}",
        archive_md,
        serde_json::to_string(&archive_json).unwrap_or_default()
    );
    let marker = crate::policies::digest::digest(&combined);
    fs::write(change_dir.join(ARCHIVE_MARKER), marker)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入归档标记失败：{e}")))?;

    // 清理展开制品：保留 archive.json/archive.md/.archived 与 run 级结果
    for entry in fs::read_dir(&change_dir)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("读取 change 目录失败：{e}")))?
    {
        let entry = entry.map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("读取 change 条目失败：{e}"))
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name != "archive.json" && name != "archive.md" && name != ARCHIVE_MARKER {
            let _ = fs::remove_file(entry.path());
        }
    }

    store.update(|s| {
        s.current_phase = "ARCHIVED".to_string();
        s.in_progress_phase = None;
        s.suggested_command = Some("sdd new <需求>".to_string());
        s.last_command = Some("sdd archive".to_string());
        s.last_error = None;
    })?;

    Ok(CommandResult {
        ok: true,
        state: "ARCHIVED".to_string(),
        exit_code: 0,
        change_id: Some(change_id),
        next: Some("sdd new <需求>".to_string()),
        data: None,
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}
