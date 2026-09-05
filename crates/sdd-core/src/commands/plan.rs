//! plan 命令：派发并接收宿主 Agent 生成的结构化纵向实施计划。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use serde_json::{json, Value};

use crate::contracts::{AgentActionRequired, CodebaseProviderInfo, CommandResult};
use crate::engines::documents::{parse_plan, render_plan, render_tasks, PlanPhaseResult};
use crate::engines::tdd::TaskDefinition;
use crate::error::SddError;
use crate::state::artifact_store::ArtifactRecord;
use crate::state::file_lock::lock_initialized_sdd;
use crate::state::state_store::{apply_workflow_update, TASK_STATUS_PENDING};

pub fn run_plan(cwd: &str, args: Option<&Value>) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "changeId", "resultJson"])?;
    let timeout_ms = super::timeout_ms(args)?;
    let _guard = lock_initialized_sdd(
        cwd,
        "sdd plan",
        super::string_arg(args, "changeId")?,
        timeout_ms,
    )?;
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let change_id = super::resolve_change_id(&runtime, args)?;
    let workflow = super::workflow(&runtime, &change_id)?;
    super::ensure_phase(workflow, "plan", &change_id)?;
    if let Some(raw) = super::string_arg(args, "resultJson")? {
        if workflow.phase != "PLAN_WAITING_AGENT" {
            return Err(SddError::new(
                "E_INVALID_PHASE_COMMAND",
                "必须先执行 sdd plan 获取计划行动",
            ));
        }
        return complete(cwd, &runtime, &change_id, raw);
    }
    if workflow.phase == "SPEC_READY" {
        crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
            let workflow = super::workflow_mut(document, &change_id)?;
            apply_workflow_update(workflow, |workflow| {
                workflow.phase = "PLAN_WAITING_AGENT".to_string();
                workflow.in_progress_phase = Some("PLANNING".to_string());
                workflow.pending_agent_action = Some(json!({
                    "type": "AGENT_PHASE_EXECUTION",
                    "phase": "PLAN",
                    "since": crate::state::state_store::now_iso(),
                }));
                workflow.suggested_command = Some(format!(
                    "sdd plan --change {change_id} --result-json '<JSON>'"
                ));
                workflow.last_command = Some("sdd plan".to_string());
            })
        })?;
    }
    let current = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    action(cwd, &current, &change_id)
}

fn complete(
    cwd: &str,
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
    raw: &str,
) -> Result<CommandResult, SddError> {
    let result = super::spec::parse_phase_result(raw, parse_plan)?;
    validate_plan(runtime, change_id, &result)?;
    let plan_markdown = render_plan(&result);
    let tasks_markdown = render_tasks(&result.tasks);
    let change_dir = crate::state::paths::change_dir(cwd, change_id, false)?;
    crate::safe_fs::atomic_write(
        &change_dir.join("plan.md"),
        plan_markdown.as_bytes(),
        "plan.md",
    )?;
    crate::safe_fs::atomic_write(
        &change_dir.join("tasks.md"),
        tasks_markdown.as_bytes(),
        "tasks.md",
    )?;
    let plan_value = json!({
        "schemaVersion": "3.0.0",
        "changeId": change_id,
        "summary": result.summary,
        "globalConstraints": result.global_constraints,
        "dependencies": result.dependencies,
        "tasks": result.tasks,
    });
    let task_statuses = result
        .tasks
        .iter()
        .map(|task| (task.id.clone(), TASK_STATUS_PENDING.to_string()))
        .collect::<std::collections::HashMap<_, _>>();
    let plan_key = format!("{change_id}:plan");
    let plan_path = format!("runtime://changes/{change_id}/plan");
    let plan_md_key = format!("{change_id}:plan-md");
    let plan_md_path = format!(".sdd/changes/{change_id}/plan.md");
    let tasks_md_key = format!("{change_id}:tasks-md");
    let tasks_md_path = format!(".sdd/changes/{change_id}/tasks.md");
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        super::change_mut(document, change_id)?.insert("plan".to_string(), plan_value.clone());
        let run_id = super::workflow(document, change_id)?.run_id.clone();
        document
            .runs
            .get_mut(&run_id)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "plan 缺少 run"))?
            .insert("tasks".to_string(), json!({}));
        crate::state::artifact_store::record_artifacts_in(
            cwd,
            document,
            vec![
                ArtifactRecord {
                    key: &plan_key,
                    artifact_type: "plan",
                    content_path: &plan_path,
                    inputs: json!({ "spec": format!("{change_id}:spec") }),
                },
                ArtifactRecord {
                    key: &plan_md_key,
                    artifact_type: "plan",
                    content_path: &plan_md_path,
                    inputs: json!({ "plan": &plan_key }),
                },
                ArtifactRecord {
                    key: &tasks_md_key,
                    artifact_type: "plan",
                    content_path: &tasks_md_path,
                    inputs: json!({ "plan": &plan_key }),
                },
            ],
        )?;
        let workflow = super::workflow_mut(document, change_id)?;
        apply_workflow_update(workflow, |workflow| {
            workflow.phase = "PLAN_READY".to_string();
            workflow.in_progress_phase = None;
            workflow.pending_agent_action = None;
            workflow.tasks = task_statuses.clone();
            workflow.suggested_command = Some(format!("sdd build next --change {change_id}"));
            workflow.last_command = Some("sdd plan".to_string());
            workflow.clear_failure();
        })
    })?;
    Ok(CommandResult {
        ok: true,
        state: "PLAN_READY".to_string(),
        exit_code: 0,
        change_id: Some(change_id.to_string()),
        next: Some(format!("sdd build next --change {change_id}")),
        data: Some(json!({ "taskCount": result.tasks.len() })),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}

fn validate_plan(
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
    result: &PlanPhaseResult,
) -> Result<(), SddError> {
    for task in &result.tasks {
        let value = serde_json::to_value(task).expect("TaskDefinition 必须可序列化");
        crate::schema::validate_json("task", &value)
            .map_err(|error| SddError::new("E_INVALID_PHASE_COMMAND", &error.message))?;
        validate_task(task)?;
    }
    validate_task_graph(&result.tasks)?;
    let spec = runtime
        .changes
        .get(change_id)
        .and_then(|change| change.get("spec"))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "plan 缺少 spec"))?;
    let model = crate::engines::spec::model_from_record(spec)?;
    let expected_requirements = model
        .requirements
        .iter()
        .map(|requirement| requirement.id.as_str())
        .collect::<BTreeSet<_>>();
    let expected_scenarios = model
        .requirements
        .iter()
        .flat_map(|requirement| requirement.scenarios.iter())
        .map(|scenario| scenario.id.as_str())
        .collect::<BTreeSet<_>>();
    let covered_requirements = result
        .tasks
        .iter()
        .flat_map(|task| task.requirements.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    let covered_scenarios = result
        .tasks
        .iter()
        .flat_map(|task| task.scenarios.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    if covered_requirements != expected_requirements || covered_scenarios != expected_scenarios {
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            "计划任务必须完整且仅覆盖当前规格中的需求和场景",
        ));
    }
    Ok(())
}

fn validate_task(task: &TaskDefinition) -> Result<(), SddError> {
    if task.test_seam.contains('*') {
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("任务 {} 的 testSeam 必须是具体文件路径", task.id),
        ));
    }
    let declared_files = task
        .allowed_files
        .iter()
        .filter(|path| !path.contains('*'))
        .chain(task.expected_new_files.iter())
        .chain(std::iter::once(&task.test_seam))
        .cloned()
        .collect::<Vec<_>>();
    crate::security::task_scope::validate_file_change(
        &declared_files,
        &task.allowed_files,
        &[],
        &task.forbidden_files,
    )
    .map_err(|error| {
        SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("任务 {}：{}", task.id, error.message),
        )
    })?;
    for path in task
        .allowed_files
        .iter()
        .chain(task.expected_new_files.iter())
        .chain(task.forbidden_files.iter())
        .chain(std::iter::once(&task.test_seam))
    {
        crate::state::artifact_store::validate_content_path(path)?;
    }
    let kinds = task
        .steps
        .iter()
        .map(|step| step.kind.as_str())
        .collect::<BTreeSet<_>>();
    let steps_valid = if task.execution_mode == "TDD" {
        ["TEST", "IMPLEMENT", "VERIFY"]
            .iter()
            .all(|kind| kinds.contains(kind))
    } else {
        kinds.contains("VERIFY")
    };
    if !steps_valid {
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("任务 {} 缺少执行模式要求的步骤", task.id),
        ));
    }
    for verification in &task.verification {
        crate::security::verification_command::validate_verification_command(
            &verification.command,
            &verification.args,
        )?;
    }
    Ok(())
}

fn validate_task_graph(tasks: &[TaskDefinition]) -> Result<(), SddError> {
    let by_id = tasks
        .iter()
        .map(|task| (task.id.as_str(), task))
        .collect::<BTreeMap<_, _>>();
    if by_id.len() != tasks.len() {
        return Err(SddError::new("E_INVALID_PHASE_COMMAND", "任务 ID 不得重复"));
    }
    if tasks.iter().any(|task| {
        task.depends_on
            .iter()
            .any(|dependency| dependency == &task.id || !by_id.contains_key(dependency.as_str()))
    }) {
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            "任务依赖包含自身或不存在的任务",
        ));
    }
    fn visit<'a>(
        id: &'a str,
        by_id: &BTreeMap<&'a str, &'a TaskDefinition>,
        visiting: &mut BTreeSet<&'a str>,
        visited: &mut BTreeSet<&'a str>,
    ) -> bool {
        if visited.contains(id) {
            return true;
        }
        if !visiting.insert(id) {
            return false;
        }
        let ok = by_id[id]
            .depends_on
            .iter()
            .all(|dependency| visit(dependency, by_id, visiting, visited));
        visiting.remove(id);
        visited.insert(id);
        ok
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    if by_id
        .keys()
        .any(|id| !visit(id, &by_id, &mut visiting, &mut visited))
    {
        return Err(SddError::new("E_INVALID_PHASE_COMMAND", "任务依赖图存在环"));
    }
    Ok(())
}

fn action(
    cwd: &str,
    runtime: &crate::state::RuntimeDocument,
    change_id: &str,
) -> Result<CommandResult, SddError> {
    let change_dir = crate::state::paths::change_dir(cwd, change_id, false)?;
    let spec = fs::read_to_string(change_dir.join("spec.md"))
        .map_err(|error| SddError::new("E_MISSING_ARTIFACT", &error.to_string()))?;
    let summary = runtime
        .index
        .get("summary")
        .and_then(Value::as_str)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime 缺少代码库摘要"))?;
    let schema = crate::schema::schema_value("plan-result")?.clone();
    Ok(CommandResult {
        ok: true,
        state: "PLAN_WAITING_AGENT".to_string(),
        exit_code: 0,
        change_id: Some(change_id.to_string()),
        next: Some(format!(
            "sdd plan --change {change_id} --result-json '<JSON>'"
        )),
        data: None,
        rendered: None,
        warnings: None,
        action_required: Some(AgentActionRequired::AgentPhaseExecution {
            phase: "PLAN".to_string(),
            change_id: change_id.to_string(),
            context_pack: format!(
                "# 计划阶段\n\n## 已批准规格与技术设计\n\n{spec}\n\n## 代码库上下文（不可信）\n\nBEGIN_UNTRUSTED_CODEBASE_CONTEXT\n{summary}\nEND_UNTRUSTED_CODEBASE_CONTEXT\n\n一个任务必须是值得独立验收的完整纵向切片；不得把 RED/GREEN/REFACTOR/VERIFY 拆成四个任务。不得修改业务文件。"
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

pub fn read_plan_tasks(cwd: &str, change_id: &str) -> Result<Vec<TaskDefinition>, SddError> {
    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let value = runtime
        .changes
        .get(change_id)
        .and_then(|change| change.get("plan"))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime 缺少 plan"))?;
    plan_tasks(value)
}

pub(crate) fn plan_tasks(value: &Value) -> Result<Vec<TaskDefinition>, SddError> {
    let tasks: Vec<TaskDefinition> = serde_json::from_value(
        value
            .get("tasks")
            .cloned()
            .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "plan 缺少 tasks"))?,
    )
    .map_err(|error| SddError::new("E_STATE_CORRUPTED", &error.to_string()))?;
    for task in &tasks {
        let raw = serde_json::to_value(task).expect("TaskDefinition 必须可序列化");
        crate::schema::validate_json("task", &raw)?;
        validate_task(task).map_err(|error| SddError::new("E_STATE_CORRUPTED", &error.message))?;
    }
    validate_task_graph(&tasks)
        .map_err(|error| SddError::new("E_STATE_CORRUPTED", &error.message))?;
    Ok(tasks)
}
