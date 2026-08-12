//! change 命令：直接更新活动需求，并清除由旧需求生成的派生制品。

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::commands::new::render_spec_document;
use crate::contracts::CommandResult;
use crate::engines::spec::spec_engine::{GenerateSpecInput, SpecEngine};
use crate::error::SddError;
use crate::git::isolation::validate_change_id;
use crate::policies::digest::digest;
use crate::state::file_lock::lock_sdd;
use crate::state::runtime_store::RuntimeStore;
use crate::state::state_store::{now_iso, StateStore};

const DERIVED_DOCUMENTS: [&str; 4] = ["design.md", "plan.md", "tasks.md", "archive.md"];

#[derive(Debug)]
struct ChangeArgs {
    change_id: String,
    requirement: String,
    answers: HashMap<String, String>,
    timeout_ms: Option<u64>,
}

impl ChangeArgs {
    fn from_json(args: Option<&Value>) -> Result<Self, SddError> {
        let args = args.cloned().unwrap_or_else(|| json!({}));
        let change_id = args
            .get("changeId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| SddError::new("E_MISSING_CHANGE", "必须提供目标变更 ID"))?
            .to_string();
        validate_change_id(&change_id)?;

        let requirement = args
            .get("requirement")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| SddError::new("E_INVALID_REQUIREMENT", "需求内容不能为空"))?
            .to_string();

        let answers = parse_answers(args.get("answers"))?;
        let timeout_ms = args
            .get("timeout")
            .and_then(Value::as_f64)
            .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
            .map(|seconds| (seconds * 1000.0) as u64);

        Ok(Self {
            change_id,
            requirement,
            answers,
            timeout_ms,
        })
    }
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
    let parsed = ChangeArgs::from_json(args)?;
    let _guard = lock_sdd(
        cwd,
        "sdd change",
        Some(&parsed.change_id),
        parsed.timeout_ms,
    )?;
    let state_store = StateStore::new(cwd.to_string());
    let state_before = state_store.read()?;

    if state_before.current_change_id.as_deref() != Some(parsed.change_id.as_str()) {
        return Err(reject(
            cwd,
            state_before.current_run_id.as_deref(),
            &parsed.change_id,
            "E_MISSING_CHANGE",
            &format!("指定变更 {} 不是当前活动变更", parsed.change_id),
        ));
    }
    if state_before.current_phase == "ARCHIVED" {
        return Err(reject(
            cwd,
            state_before.current_run_id.as_deref(),
            &parsed.change_id,
            "E_ARCHIVED_READONLY",
            "已归档的变更为只读状态",
        ));
    }

    let change_dir = change_directory(cwd, &parsed.change_id);
    if !change_dir.is_dir() {
        return Err(reject(
            cwd,
            state_before.current_run_id.as_deref(),
            &parsed.change_id,
            "E_MISSING_CHANGE",
            &format!("变更目录不存在：{}", parsed.change_id),
        ));
    }

    let mut runtime = RuntimeStore::new(cwd.to_string()).read()?;
    let current_change = runtime
        .changes
        .get(&parsed.change_id)
        .cloned()
        .ok_or_else(|| {
            reject(
                cwd,
                state_before.current_run_id.as_deref(),
                &parsed.change_id,
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
            cwd,
            state_before.current_run_id.as_deref(),
            &parsed.change_id,
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
        .map_err(|error| SddError::new("E_STATE_CORRUPTED", &format!("生成新规格失败：{error}")))?;

    let old_documents = read_documents(&change_dir)?;
    if !old_documents.contains_key("spec.md") {
        return Err(SddError::new("E_MISSING_ARTIFACT", "变更目录缺少 spec.md"));
    }
    let spec_document = render_spec_document(&parsed.requirement, &answers, &artifacts.spec);
    let mut new_documents = old_documents.clone();
    for name in DERIVED_DOCUMENTS {
        new_documents.remove(name);
    }
    new_documents.insert("spec.md".to_string(), spec_document.clone());
    new_documents.insert("proposal.md".to_string(), artifacts.proposal);

    let mut next_change = current_change;
    let next_spec = json!({
        "schemaVersion": "2.0.0",
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
    runtime
        .changes
        .insert(parsed.change_id.clone(), next_change);
    update_artifacts(&mut runtime.artifacts, &parsed.change_id, &spec_document)?;

    let mut next_state = runtime.state.clone();
    next_state.previous_phase = Some(next_state.current_phase.clone());
    next_state.current_phase = "SPEC_READY".to_string();
    next_state.in_progress_phase = None;
    next_state.suggested_command = Some("sdd design".to_string());
    next_state.last_command = Some("sdd change".to_string());
    next_state.last_error = None;
    next_state.failed_command = None;
    next_state.failed_reason = None;
    next_state.interrupted_command = None;
    next_state.recoverable = true;
    next_state.tasks.clear();
    next_state.artifacts.clear();
    next_state.pending_agent_task = None;
    runtime.state = next_state;
    append_event(
        &mut runtime.loop_state,
        runtime.state.current_run_id.as_deref(),
        "REQUIREMENT_REVISED",
        &parsed.change_id,
        None,
    );

    write_documents(&change_dir, &old_documents, &new_documents)?;
    if let Err(error) = RuntimeStore::new(cwd.to_string()).write(&runtime) {
        restore_documents(&change_dir, &old_documents);
        return Err(error);
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

fn change_directory(cwd: &str, change_id: &str) -> PathBuf {
    PathBuf::from(cwd)
        .join(".sdd")
        .join("changes")
        .join(change_id)
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
    let suffix = format!("{}.tmp", std::process::id());
    let mut temporary = Vec::new();
    for (name, content) in after {
        let temp = change_dir.join(format!(".{name}.{suffix}"));
        if let Err(error) = write_file(&temp, content) {
            cleanup_temporary(&temporary);
            let _ = fs::remove_file(temp);
            return Err(error);
        }
        temporary.push((temp, change_dir.join(name)));
    }
    for (temp, path) in &temporary {
        if let Err(error) = fs::rename(temp, path) {
            cleanup_temporary(&temporary);
            restore_documents(change_dir, before);
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                &format!("提交变更文档失败：{error}"),
            ));
        }
    }
    for name in before.keys().filter(|name| !after.contains_key(*name)) {
        if let Err(error) = fs::remove_file(change_dir.join(name)) {
            if error.kind() != std::io::ErrorKind::NotFound {
                restore_documents(change_dir, before);
                return Err(SddError::new(
                    "E_STATE_CORRUPTED",
                    &format!("删除过期文档 {name} 失败：{error}"),
                ));
            }
        }
    }
    Ok(())
}

fn cleanup_temporary(temporary: &[(PathBuf, PathBuf)]) {
    for (path, _) in temporary {
        let _ = fs::remove_file(path);
    }
}

fn restore_documents(change_dir: &Path, documents: &BTreeMap<String, String>) {
    for (name, content) in documents {
        let _ = fs::write(change_dir.join(name), content);
    }
    if let Ok(entries) = fs::read_dir(change_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if name.ends_with(".md") && !documents.contains_key(name) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn update_artifacts(
    artifacts: &mut Value,
    change_id: &str,
    spec_content: &str,
) -> Result<(), SddError> {
    if !artifacts.is_object() {
        *artifacts = json!({ "schemaVersion": "2.0.0", "artifacts": {} });
    }
    let object = artifacts
        .as_object_mut()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "runtime artifacts 必须是对象"))?;
    let entries = object
        .entry("artifacts".to_string())
        .or_insert_with(|| json!({}));
    let entries = entries.as_object_mut().ok_or_else(|| {
        SddError::new(
            "E_STATE_CORRUPTED",
            "runtime artifacts.artifacts 必须是对象",
        )
    })?;
    let prefix = format!("{change_id}:");
    entries.retain(|key, _| !key.starts_with(&prefix));
    let item = json!({
        "type": "spec",
        "hash": digest(spec_content),
        "contentPath": format!(".sdd/changes/{change_id}/spec.md"),
        "status": "READY",
        "inputs": { "source": "sdd change" },
    });
    crate::schema::validate_json("artifact", &item)?;
    entries.insert(format!("{change_id}:spec"), item);
    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<(), SddError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            SddError::new("E_STATE_CORRUPTED", &format!("创建文件目录失败：{error}"))
        })?;
    }
    fs::write(path, content)
        .map_err(|error| SddError::new("E_STATE_CORRUPTED", &format!("写入文件失败：{error}")))
}

fn append_event(
    loop_state: &mut Value,
    run_id: Option<&str>,
    event_type: &str,
    change_id: &str,
    reason: Option<&str>,
) {
    let Some(run_id) = run_id else {
        return;
    };
    if !loop_state.is_object() {
        *loop_state = json!({ "runs": {}, "events": {} });
    }
    let object = loop_state.as_object_mut().expect("loop state object");
    let events = object.entry("events").or_insert_with(|| json!({}));
    if !events.is_object() {
        *events = json!({});
    }
    let run_events = events
        .as_object_mut()
        .expect("events object")
        .entry(run_id.to_string())
        .or_insert_with(|| json!([]));
    if !run_events.is_array() {
        *run_events = json!([]);
    }
    let mut event = json!({
        "schemaVersion": "1.0.0",
        "eventId": make_event_id(),
        "runId": run_id,
        "type": event_type,
        "changeId": change_id,
        "createdAt": now_iso(),
    });
    if let Some(reason) = reason {
        event["reason"] = json!(reason);
    }
    run_events
        .as_array_mut()
        .expect("run events array")
        .push(event);
}

fn reject(cwd: &str, run_id: Option<&str>, change_id: &str, code: &str, message: &str) -> SddError {
    let reason = format!("{code}: {message}");
    let _ = RuntimeStore::new(cwd.to_string()).update(|runtime| {
        append_event(
            &mut runtime.loop_state,
            run_id,
            "REQUIREMENT_REJECTED",
            change_id,
            Some(&reason),
        );
    });
    SddError::new(code, message).with_next("sdd change")
}

fn make_event_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("event-{nanos}")
}
