//! change 命令：直接更新活动需求，并清除由旧需求生成的派生制品。

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::commands::new::{render_spec_document, validate_requirement_length};
use crate::contracts::CommandResult;
use crate::engines::spec::spec_engine::{GenerateSpecInput, SpecEngine};
use crate::error::SddError;
use crate::git::isolation::validate_change_id;
use crate::state::artifact_store::ArtifactRecord;
use crate::state::file_lock::lock_initialized_sdd;
use crate::state::runtime_store::RuntimeStore;

const DERIVED_DOCUMENTS: [&str; 4] = ["design.md", "plan.md", "tasks.md", "archive.md"];

#[derive(Debug)]
struct ChangeArgs {
    change_id: String,
    requirement: String,
    answers: HashMap<String, String>,
}

impl ChangeArgs {
    fn from_json(args: Option<&Value>, requirement: Option<String>) -> Result<Self, SddError> {
        let empty = Value::Null;
        let args = args.unwrap_or(&empty);
        let change_id = args
            .get("changeId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| SddError::new("E_MISSING_CHANGE", "必须提供目标变更 ID"))?
            .to_string();
        validate_change_id(&change_id)?;

        let requirement = requirement
            .ok_or_else(|| SddError::new("E_INVALID_REQUIREMENT", "需求内容不能为空"))?;

        let answers = parse_answers(args.get("answers"))?;
        Ok(Self {
            change_id,
            requirement,
            answers,
        })
    }
}

fn prevalidate_requirement(args: Option<&Value>) -> Result<Option<String>, SddError> {
    let Some(value) = args.and_then(|args| args.get("requirement")) else {
        return Ok(None);
    };
    let requirement = value
        .as_str()
        .map(str::trim)
        .filter(|requirement| !requirement.is_empty())
        .ok_or_else(|| SddError::new("E_INVALID_REQUIREMENT", "需求内容不能为空"))?
        .to_string();
    validate_requirement_length(&requirement)?;
    Ok(Some(requirement))
}

fn parse_answers(value: Option<&Value>) -> Result<HashMap<String, String>, SddError> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };
    let object = value
        .as_object()
        .ok_or_else(|| SddError::new("E_INVALID_PHASE_COMMAND", "answers 必须是 JSON 对象"))?;
    object
        .iter()
        .map(|(key, value)| {
            let answer = value.as_str().ok_or_else(|| {
                SddError::new(
                    "E_INVALID_PHASE_COMMAND",
                    &format!("answers.{key} 必须是字符串"),
                )
            })?;
            Ok((key.clone(), answer.to_string()))
        })
        .collect()
}

/// 执行需求修改。Git 负责历史，SDD 只保留当前文档和当前机器状态。
pub fn run_change(
    cwd: &str,
    args: Option<&Value>,
    engine: &SpecEngine,
) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "changeId", "requirement", "answers"])?;
    let requirement = prevalidate_requirement(args)?;
    let timeout_ms = super::timeout_ms(args)?;
    let requested_change_id = super::string_arg(args, "changeId")?;
    let _guard = lock_initialized_sdd(cwd, "sdd change", requested_change_id, timeout_ms)?;
    let runtime = RuntimeStore::new(cwd.to_string()).read()?;
    let state_before = runtime.state.clone();
    super::ensure_phase(cwd, &state_before, "change", args)?;
    let parsed = ChangeArgs::from_json(args, requirement)?;

    let change_dir = crate::state::paths::change_dir(cwd, &parsed.change_id, false)?;
    if !change_dir.is_dir() {
        return Err(reject(
            "E_MISSING_CHANGE",
            &format!("变更目录不存在：{}", parsed.change_id),
        ));
    }

    let current_change = runtime
        .changes
        .get(&parsed.change_id)
        .cloned()
        .ok_or_else(|| {
            reject(
                "E_MISSING_CHANGE",
                &format!("runtime.json 中不存在变更：{}", parsed.change_id),
            )
        })?;
    let current_spec = current_change
        .get("spec")
        .and_then(Value::as_object)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少当前 spec"))?;
    let mut answers = read_answers(current_spec.get("answers"))?;
    answers.extend(parsed.answers);

    let analysis = engine.analyze(&parsed.requirement, &answers);
    let blockers: Vec<String> = analysis
        .questions
        .iter()
        .filter(|question| question.severity == "BLOCKER")
        .map(|question| format!("{}：{}", question.id, question.question))
        .collect();
    if !blockers.is_empty() {
        return Err(reject(
            "E_UNRESOLVED_BLOCKER",
            &format!("需求仍存在未回答阻塞问题：{}", blockers.join("；")),
        ));
    }

    let codebase_summary = runtime
        .index
        .get("summary")
        .and_then(Value::as_str)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少代码库摘要"))?
        .to_string();
    let artifacts = engine
        .generate(&GenerateSpecInput {
            requirement: parsed.requirement.clone(),
            codebase_summary,
            answers: answers.clone(),
        })
        .map_err(|error| {
            SddError::new("E_GENERATION_FAILED", &format!("生成新规格失败：{error}"))
        })?;

    let old_documents = read_documents(&change_dir)?;
    if !old_documents.contains_key("spec.md") {
        return Err(SddError::new("E_MISSING_ARTIFACT", "变更目录缺少 spec.md"));
    }
    let spec_document = render_spec_document(&parsed.requirement, &answers, &artifacts.model)?;
    let mut new_documents = old_documents.clone();
    for name in DERIVED_DOCUMENTS {
        new_documents.remove(name);
    }
    new_documents.insert("spec.md".to_string(), spec_document);
    new_documents.insert("proposal.md".to_string(), artifacts.proposal);

    let mut next_change = current_change;
    let next_spec = json!({
        "schemaVersion": crate::engines::spec::SPEC_SCHEMA_VERSION,
        "status": "READY",
        "requirement": parsed.requirement,
        "impact": artifacts.impact,
        "answers": answers,
        "model": artifacts.model,
    });
    let next_change_object = next_change
        .as_object_mut()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "变更运行数据必须是对象"))?;
    next_change_object.insert("spec".to_string(), next_spec);
    for field in ["design", "plan", "reports", "archive"] {
        next_change_object.remove(field);
    }
    write_documents(&change_dir, &old_documents, &new_documents)?;
    let run_id = state_before
        .current_run_id
        .as_deref()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "活动变更缺少 currentRunId"))?;
    let artifact_key = format!("{}:spec", parsed.change_id);
    let content_path = format!(".sdd/changes/{}/spec.md", parsed.change_id);
    let event = requirement_revised_event(run_id, &parsed.change_id)?;
    let runtime_result = RuntimeStore::new(cwd.to_string()).try_update(|document| {
        document
            .changes
            .insert(parsed.change_id.clone(), next_change);
        let entries = document
            .artifacts
            .get_mut("artifacts")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                SddError::new(
                    "E_STATE_CORRUPTED",
                    "runtime artifacts.artifacts 必须是对象",
                )
            })?;
        let prefix = format!("{}:", parsed.change_id);
        entries.retain(|key, _| !key.starts_with(&prefix));
        crate::state::artifact_store::record_artifacts_in(
            cwd,
            document,
            vec![ArtifactRecord {
                key: &artifact_key,
                artifact_type: "spec",
                content_path: &content_path,
                inputs: json!({ "source": "sdd change" }),
            }],
        )?;
        crate::state::state_store::apply_state_update(&mut document.state, |state| {
            state.current_phase = "SPEC_READY".to_string();
            state.in_progress_phase = None;
            state.suggested_command = Some("sdd design".to_string());
            state.last_command = Some("sdd change".to_string());
            state.clear_failure();
            state.tasks.clear();
            state.pending_agent_task = None;
        })?;
        append_requirement_event(document, run_id, event)?;
        Ok(())
    });
    if let Err(error) = runtime_result {
        return Err(with_recovery(
            error,
            restore_documents(&change_dir, &old_documents),
        ));
    }

    let removed_documents: Vec<String> = old_documents
        .keys()
        .filter(|name| !new_documents.contains_key(*name))
        .cloned()
        .collect();
    let document_names: Vec<String> = new_documents.keys().cloned().collect();
    Ok(CommandResult {
        ok: true,
        state: "SPEC_READY".to_string(),
        exit_code: 0,
        change_id: Some(parsed.change_id),
        next: Some("sdd design".to_string()),
        data: Some(json!({
            "documents": document_names,
            "removedDocuments": removed_documents,
            "changedDocumentCount": new_documents.len(),
        })),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}

fn read_documents(change_dir: &Path) -> Result<BTreeMap<String, String>, SddError> {
    let mut documents = BTreeMap::new();
    let entries = fs::read_dir(change_dir).map_err(|error| {
        SddError::new("E_MISSING_CHANGE", &format!("读取变更目录失败：{error}"))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            SddError::new("E_STATE_CORRUPTED", &format!("读取变更文件失败：{error}"))
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "变更文档名称不是有效 UTF-8"))?
            .to_string();
        crate::safe_fs::reject_symlink(&path, &format!("变更文档 {name}"))?;
        let content = fs::read_to_string(&path).map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("读取文档 {name} 失败：{error}"),
            )
        })?;
        documents.insert(name, content);
    }
    Ok(documents)
}

fn read_answers(value: Option<&Value>) -> Result<HashMap<String, String>, SddError> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };
    let object = value
        .as_object()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "runtime spec.answers 必须是对象"))?;
    object
        .iter()
        .map(|(key, value)| {
            value
                .as_str()
                .map(|answer| (key.clone(), answer.to_string()))
                .ok_or_else(|| {
                    SddError::new("E_STATE_CORRUPTED", "runtime spec.answers 含非字符串值")
                })
        })
        .collect()
}

fn write_documents(
    change_dir: &Path,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Result<(), SddError> {
    for (name, content) in after {
        let path = change_dir.join(name);
        if let Err(error) =
            crate::safe_fs::atomic_write(&path, content.as_bytes(), &format!("变更文档 {name}"))
        {
            return Err(with_recovery(error, restore_documents(change_dir, before)));
        }
    }
    for name in before.keys().filter(|name| !after.contains_key(*name)) {
        if let Err(error) = fs::remove_file(change_dir.join(name)) {
            if error.kind() != std::io::ErrorKind::NotFound {
                let cause = SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("删除过期文档 {name} 失败：{error}"),
                );
                return Err(with_recovery(cause, restore_documents(change_dir, before)));
            }
        }
    }
    Ok(())
}

fn restore_documents(
    change_dir: &Path,
    documents: &BTreeMap<String, String>,
) -> Result<(), SddError> {
    for (name, content) in documents {
        crate::safe_fs::atomic_write(
            &change_dir.join(name),
            content.as_bytes(),
            &format!("回滚文档 {name}"),
        )?;
    }
    let entries = fs::read_dir(change_dir).map_err(|error| {
        SddError::new("E_STATE_CORRUPTED", &format!("读取回滚目录失败：{error}"))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            SddError::new(
                "E_STATE_CORRUPTED",
                &format!("读取回滚目录条目失败：{error}"),
            )
        })?;
        let path = entry.path();
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if name.ends_with(".md") && !documents.contains_key(name) {
            remove_if_exists(&path)?;
        }
    }
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<(), SddError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("清理文件 {} 失败：{error}", path.display()),
        )),
    }
}

fn with_recovery(mut cause: SddError, recovery: Result<(), SddError>) -> SddError {
    if let Err(error) = recovery {
        cause.message = format!("{}；恢复原文档失败：{}", cause.message, error.message);
    }
    cause
}

fn requirement_revised_event(run_id: &str, change_id: &str) -> Result<Value, SddError> {
    Ok(json!({
        "schemaVersion": "1.0.0",
        "eventId": crate::state::state_store::unique_id("event")?,
        "runId": run_id,
        "type": "REQUIREMENT_REVISED",
        "changeId": change_id,
        "createdAt": crate::state::state_store::now_iso(),
    }))
}

fn append_requirement_event(
    document: &mut crate::state::RuntimeDocument,
    run_id: &str,
    event: Value,
) -> Result<(), SddError> {
    let run = document
        .runs
        .get_mut(run_id)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "活动 run 必须是对象"))?;
    let run_events = run
        .entry("events".to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "活动 run.events 必须是数组"))?;
    run_events.push(event);
    Ok(())
}

fn reject(code: &str, message: &str) -> SddError {
    SddError::new(code, message).with_next("sdd change")
}
