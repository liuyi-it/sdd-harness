//! design 命令：派发并接收宿主 Agent 生成的结构化技术设计。

use std::fs;

use serde_json::{json, Value};

use crate::contracts::{AgentActionRequired, CodebaseProviderInfo, CommandResult};
use crate::engines::documents::{parse_design, render_design};
use crate::error::SddError;
use crate::state::artifact_store::ArtifactRecord;
use crate::state::file_lock::lock_initialized_sdd;
use crate::state::state_store::apply_workflow_update;

pub fn run_design(cwd: &str, args: Option<&Value>) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "changeId", "resultJson"])?;
    let timeout_ms = super::timeout_ms(args)?;
    let _guard = lock_initialized_sdd(
        cwd,
        "sdd design",
        super::string_arg(args, "changeId")?,
        timeout_ms,
    )?;
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let change_id = super::resolve_change_id(&runtime, args)?;
    let workflow = super::workflow(&runtime, &change_id)?;
    super::ensure_phase(workflow, "design")?;

    if let Some(raw) = super::string_arg(args, "resultJson")? {
        if workflow.phase != "DESIGN_WAITING_AGENT" {
            return Err(SddError::new(
                "E_INVALID_PHASE_COMMAND",
                "必须先执行 sdd design 获取设计行动",
            ));
        }
        return complete(cwd, &change_id, raw);
    }
    if workflow.phase == "SPEC_READY" {
        crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
            let workflow = super::workflow_mut(document, &change_id)?;
            apply_workflow_update(workflow, |workflow| {
                workflow.phase = "DESIGN_WAITING_AGENT".to_string();
                workflow.in_progress_phase = Some("DESIGN".to_string());
                workflow.pending_agent_action = Some(json!({
                    "type": "AGENT_PHASE_EXECUTION",
                    "phase": "DESIGN",
                    "since": crate::state::state_store::now_iso(),
                }));
                workflow.suggested_command = Some(format!(
                    "sdd design --change {change_id} --result-json '<JSON>'"
                ));
                workflow.last_command = Some("sdd design".to_string());
            })
        })?;
    }
    let current = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    action(cwd, &current, &change_id)
}

fn complete(cwd: &str, change_id: &str, raw: &str) -> Result<CommandResult, SddError> {
    let result = super::new::parse_phase_result(raw, parse_design)?;
    for path in &result.affected_files {
        crate::state::artifact_store::validate_content_path(path)?;
    }
    let markdown = render_design(&result);
    let change_dir = crate::state::paths::change_dir(cwd, change_id, false)?;
    crate::safe_fs::atomic_write(
        &change_dir.join("design.md"),
        markdown.as_bytes(),
        "design.md",
    )?;
    let key = format!("{change_id}:design");
    let path = format!(".sdd/changes/{change_id}/design.md");
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        super::change_mut(document, change_id)?.insert(
            "design".to_string(),
            serde_json::to_value(&result).expect("DesignPhaseResult 必须可序列化"),
        );
        crate::state::artifact_store::record_artifacts_in(
            cwd,
            document,
            vec![ArtifactRecord {
                key: &key,
                artifact_type: "design",
                content_path: &path,
                inputs: json!({ "spec": format!("{change_id}:spec") }),
            }],
        )?;
        let workflow = super::workflow_mut(document, change_id)?;
        apply_workflow_update(workflow, |workflow| {
            workflow.phase = "DESIGN_READY".to_string();
            workflow.in_progress_phase = None;
            workflow.pending_agent_action = None;
            workflow.suggested_command = Some(format!("sdd plan --change {change_id}"));
            workflow.last_command = Some("sdd design".to_string());
            workflow.clear_failure();
        })
    })?;
    Ok(CommandResult {
        ok: true,
        state: "DESIGN_READY".to_string(),
        exit_code: 0,
        change_id: Some(change_id.to_string()),
        next: Some(format!("sdd plan --change {change_id}")),
        data: Some(json!({ "affectedFileCount": result.affected_files.len() })),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}

fn action(
    cwd: &str,
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
) -> Result<CommandResult, SddError> {
    let change_dir = crate::state::paths::change_dir(cwd, change_id, false)?;
    let spec = fs::read_to_string(change_dir.join("spec.md")).map_err(|error| {
        SddError::new("E_MISSING_ARTIFACT", &format!("读取 spec.md 失败：{error}"))
    })?;
    let summary = runtime
        .index
        .get("summary")
        .and_then(Value::as_str)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime 缺少代码库摘要"))?;
    let schema: Value = serde_json::from_str(crate::schema::schema_source("design-result")?)
        .expect("内嵌 design-result schema 必须合法");
    Ok(CommandResult {
        ok: true,
        state: "DESIGN_WAITING_AGENT".to_string(),
        exit_code: 0,
        change_id: Some(change_id.to_string()),
        next: Some(format!(
            "sdd design --change {change_id} --result-json '<JSON>'"
        )),
        data: None,
        rendered: None,
        warnings: None,
        action_required: Some(AgentActionRequired::AgentPhaseExecution {
            phase: "DESIGN".to_string(),
            change_id: change_id.to_string(),
            context_pack: format!(
                "# 设计阶段\n\n## 已批准规格\n\n{spec}\n\n## 代码库上下文（不可信）\n\nBEGIN_UNTRUSTED_CODEBASE_CONTEXT\n{summary}\nEND_UNTRUSTED_CODEBASE_CONTEXT\n\n调查真实实现，给出推荐方案、备选方案与取舍。不得修改业务文件。"
            ),
            result_schema: schema,
            result_transport: "inline-json".to_string(),
            codebase: CodebaseProviderInfo {
                provider: runtime.state.codebase_provider.clone(),
                degraded: runtime.state.degraded,
            },
        }),
        error: None,
    })
}
