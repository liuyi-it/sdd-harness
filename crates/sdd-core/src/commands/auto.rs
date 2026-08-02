//! auto 命令：自动推进 SDD Loop。
//!
//! 翻译自 Node 版 `packages/core/src/commands/auto.ts` + loop-engine 的核心语义：
//! - 确定性步骤（new→design→plan→build→verify→review→archive）自动推进
//! - 遇到澄清（CLARIFYING）或 Agent 编码（BUILD_WAITING_AGENT）时暂停，
//!   返回当前状态与原因，不绕过交互边界
//! - 失败预算：单步失败即暂停（stopOnFailure）

use std::path::PathBuf;

use serde_json::json;

use crate::contracts::CommandResult;
use crate::engines::spec::spec_engine::SpecEngine;
use crate::engines::tdd::TddEngine;
use crate::error::SddError;
use crate::state::StateStore;

pub fn run_auto(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    // auto 不加顶层锁：内部各命令（new/design/plan/build…）各自获取写锁，
    // 与 Node 版 loop-engine 直接串联命令的行为一致
    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;

    // 恢复/停止/事件子命令：一期仅支持顺序推进（--resume 复用当前状态）
    if let Some(args) = args {
        if args.get("stop").and_then(|v| v.as_bool()).unwrap_or(false) {
            store.update(|s| {
                s.current_phase = "PAUSED".to_string();
                s.suggested_command = Some("sdd auto --resume".to_string());
            })?;
            return Ok(paused_result(&state.current_phase));
        }
    }

    let requirement = args
        .and_then(|a| a.get("requirement"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let spec_engine = SpecEngine::new();

    // 第一步：new（无需求且未初始化变更 → 暂停）
    let mut phase = state.current_phase.clone();
    if phase == "INDEX_READY" || phase == "CLARIFYING" || phase == "FAILED" || phase == "PAUSED" {
        let Some(requirement) = requirement.clone() else {
            return Ok(paused_result_with_reason(
                &phase,
                "auto 需要需求文本（sdd auto \"<需求>\"）",
            ));
        };
        let new_result = crate::commands::new::run_new(
            cwd,
            Some(&json!({ "requirement": requirement })),
            &spec_engine,
        )?;
        if new_result.state == "CLARIFYING" {
            return Ok(paused_result_with_reason(
                "CLARIFYING",
                "需求存在未回答的阻塞问题，请用 sdd new --answers '<JSON>' 继续",
            ));
        }
        phase = new_result.state.clone();
    }

    // 确定性步骤链：design → plan → build next →（Agent 完成全部任务后）verify → review → archive
    type StepFn = fn(&str, Option<&serde_json::Value>) -> Result<CommandResult, SddError>;
    let steps: [(&str, StepFn); 6] = [
        ("design", |cwd, _| {
            crate::commands::design::run_design(cwd, None, &TddEngine::new())
        }),
        ("plan", |cwd, _| {
            crate::commands::plan::run_plan(cwd, None, &TddEngine::new())
        }),
        ("build", |cwd, _| {
            crate::commands::build::run_build(cwd, None)
        }),
        ("verify", |cwd, _| {
            crate::commands::verify::run_verify(cwd, None)
        }),
        ("review", |cwd, _| {
            crate::commands::review::run_review(cwd, None)
        }),
        ("archive", |cwd, _| {
            crate::commands::archive::run_archive(cwd, None)
        }),
    ];

    for (name, step) in steps {
        let current = crate::commands::status::read_phase(cwd)?;
        let should_run = match name {
            "design" => current == "SPEC_READY",
            "plan" => current == "DESIGN_READY",
            // build 只在等待/可派发任务时运行；BUILD_READY（任务已全部完成）跳过
            "build" => current == "PLAN_READY" || current == "BUILD_WAITING_AGENT",
            "verify" => current == "BUILD_READY",
            "review" => current == "VERIFY_READY",
            "archive" => current == "REVIEW_READY" || current == "ARCHIVED",
            _ => false,
        };
        if !should_run {
            continue;
        }
        match step(cwd, None) {
            Ok(result) => {
                if result.state == "BUILD_WAITING_AGENT" {
                    // Agent 编码边界：暂停，返回 actionRequired
                    return Ok(result);
                }
                phase = result.state.clone();
                if result.state == "ARCHIVED" {
                    return Ok(CommandResult {
                        ok: true,
                        state: "ARCHIVED".to_string(),
                        exit_code: 0,
                        change_id: result.change_id.clone(),
                        next: Some("sdd new <需求>".to_string()),
                        data: Some(json!({ "loop": "COMPLETED" })),
                        rendered: None,
                        warnings: None,
                        action_required: None,
                        error: None,
                    });
                }
            }
            Err(e) => {
                // 暂停并记录失败
                store.update(|s| {
                    s.current_phase = "PAUSED".to_string();
                    s.failed_command = Some(format!("sdd {name}"));
                    s.failed_reason = Some(e.message.clone());
                    s.suggested_command = Some("sdd auto --resume".to_string());
                })?;
                return Ok(CommandResult {
                    ok: false,
                    state: "PAUSED".to_string(),
                    exit_code: e.exit_code,
                    change_id: None,
                    next: Some("sdd auto --resume".to_string()),
                    data: Some(json!({
                        "loop": "PAUSED",
                        "failedCommand": format!("sdd {name}"),
                        "reason": e.message,
                    })),
                    rendered: None,
                    warnings: None,
                    action_required: None,
                    error: Some(e.to_command_error()),
                });
            }
        }
    }

    Ok(paused_result(&phase))
}

fn paused_result(phase: &str) -> CommandResult {
    paused_result_with_reason(phase, "auto 在当前阶段暂停")
}

fn paused_result_with_reason(phase: &str, reason: &str) -> CommandResult {
    CommandResult {
        ok: false,
        state: phase.to_string(),
        exit_code: 0,
        change_id: None,
        next: Some("sdd auto --resume".to_string()),
        data: Some(json!({ "loop": "PAUSED", "reason": reason })),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    }
}

/// loop 运行记录（简化：记录最近一次运行的命令与状态）
pub fn record_loop_run(cwd: &str) -> Result<(), SddError> {
    let dir = PathBuf::from(cwd).join(".sdd/loop");
    std::fs::create_dir_all(&dir)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("创建 loop 目录失败：{e}")))?;
    Ok(())
}
