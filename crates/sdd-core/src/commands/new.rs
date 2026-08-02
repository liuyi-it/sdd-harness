//! new 命令：接收粗略需求、提出阻塞问题，并在信息充分后生成首批规格制品。
//!
//! 翻译自 Node 版 `packages/core/src/commands/new.ts`：
//! - 无需求（或空需求）→ E_MISSING_ARTIFACT
//! - 有需求但存在未回答的 BLOCKER 问题 → 写 CLARIFYING 状态的 spec.json 并返回问题
//! - 需求充分 → 生成 spec.md + spec.json（status=READY），状态推进 SPEC_READY

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::contracts::CommandResult;
use crate::engines::spec::spec_engine::{GenerateSpecInput, SpecEngine};
use crate::error::SddError;
use crate::git::GitInspector;
use crate::state::file_lock::lock_sdd;
use crate::state::{StateStore, WorkflowState};

/// spec.json 的 schema 版本（与 Node 版一致）
const SPEC_SCHEMA_VERSION: &str = "2.0.0";

pub struct NewArgs {
    pub requirement: Option<String>,
    pub change_id: Option<String>,
    pub answers: HashMap<String, String>,
    pub non_interactive: bool,
}

impl NewArgs {
    pub fn from_json(args: Option<&serde_json::Value>) -> Result<Self, SddError> {
        let args = args.cloned().unwrap_or(serde_json::Value::Null);
        let answers = match args.get("answers") {
            None => HashMap::new(),
            Some(value) => value
                .as_object()
                .ok_or_else(|| {
                    SddError::new("E_INVALID_PHASE_COMMAND", "answers 必须是 JSON 对象")
                })?
                .iter()
                .map(|(key, value)| {
                    value
                        .as_str()
                        .map(|answer| (key.clone(), answer.to_string()))
                        .ok_or_else(|| {
                            SddError::new(
                                "E_INVALID_PHASE_COMMAND",
                                &format!("answers.{key} 必须是字符串"),
                            )
                        })
                })
                .collect::<Result<HashMap<_, _>, _>>()?,
        };
        Ok(Self {
            requirement: args
                .get("requirement")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            change_id: args
                .get("changeId")
                .and_then(|v| v.as_str())
                .map(String::from),
            answers,
            non_interactive: args
                .get("nonInteractive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }
}

pub fn run_new(
    cwd: &str,
    args: Option<&serde_json::Value>,
    engine: &SpecEngine,
) -> Result<CommandResult, SddError> {
    let parsed = NewArgs::from_json(args)?;
    let timeout_ms = args
        .and_then(|a| a.get("timeout"))
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0) as u64);
    let _guard = lock_sdd(cwd, "sdd new", parsed.change_id.as_deref(), timeout_ms)?;

    let store = StateStore::new(cwd.to_string());
    let state = store.read()?;

    if let (Some(requested), Some(active)) = (
        parsed.change_id.as_deref(),
        state.current_change_id.as_deref(),
    ) {
        if requested != active && state.current_phase != "ARCHIVED" {
            return Err(SddError::new(
                "E_MISSING_CHANGE",
                &format!("指定变更 {requested} 不是当前活动变更 {active}"),
            ));
        }
    }

    // 阶段前置检查（对齐 new.ts）：仅 INDEX_READY / CLARIFYING / ARCHIVED 可开启变更
    let continuing = state.current_phase == "CLARIFYING"
        || state.current_phase == "SPEC_READY"
        || state.current_phase == "FAILED";
    if state.current_phase != "INDEX_READY"
        && state.current_phase != "CLARIFYING"
        && state.current_phase != "ARCHIVED"
        && !continuing
    {
        let code = if state.current_change_id.is_some() {
            "E_ACTIVE_CHANGE_EXISTS"
        } else {
            "E_INVALID_PHASE_COMMAND"
        };
        return Err(SddError::new(
            code,
            &format!("无法在 {} 状态下开启新的变更", state.current_phase),
        )
        .with_next(state.suggested_command.as_deref().unwrap_or("sdd status")));
    }
    if state.current_phase == "NOT_INITIALIZED" {
        return Err(
            SddError::new("E_NOT_INITIALIZED", "请先运行 sdd init 再执行其他命令")
                .with_next("sdd init"),
        );
    }

    let change_id = if continuing {
        state
            .current_change_id
            .clone()
            .unwrap_or_else(make_change_id)
    } else {
        parsed.change_id.clone().unwrap_or_else(make_change_id)
    };
    crate::git::isolation::validate_change_id(&change_id)?;
    let run_id = if continuing {
        state.current_run_id.clone().unwrap_or_else(make_run_id)
    } else {
        state
            .active_loop
            .as_ref()
            .and_then(|active| active.get("runId"))
            .and_then(|run_id| run_id.as_str())
            .map(String::from)
            .unwrap_or_else(make_run_id)
    };
    crate::state::state_store::validate_run_id(&run_id)?;

    let run_dir = PathBuf::from(cwd).join(".sdd/runs").join(&run_id);
    let change_dir = PathBuf::from(cwd).join(".sdd/changes").join(&change_id);
    if !continuing
        && change_dir
            .read_dir()
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
    {
        return Err(SddError::new(
            "E_ACTIVE_CHANGE_EXISTS",
            &format!("变更目录已存在且非空：{change_id}"),
        ));
    }
    fs::create_dir_all(&run_dir)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("创建 run 目录失败：{e}")))?;
    fs::create_dir_all(&change_dir)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("创建 change 目录失败：{e}")))?;

    let mut workspace = if continuing {
        state.workspace.clone()
    } else if crate::git::GitIsolationManager::enabled(cwd)? {
        let handle = crate::git::GitIsolationManager::ensure_worktree(cwd, &change_id)?;
        Some(crate::state::state_store::WorkspaceInfo {
            branch_name: Some(handle.branch),
            worktree_path: Some(handle.worktree_path),
            baseline_commit: handle.baseline_commit,
            ..Default::default()
        })
    } else if GitInspector::is_git_repo(cwd) {
        Some(GitInspector::snapshot(cwd).map(|snapshot| {
            crate::state::state_store::WorkspaceInfo {
                branch_name: None,
                worktree_path: None,
                baseline_commit: snapshot.head,
                ..Default::default()
            }
        })?)
    } else {
        None
    };
    if !continuing {
        if let Some(info) = workspace.as_mut() {
            let business_cwd = info.worktree_path.as_deref().unwrap_or(cwd);
            info.baseline_changed_files = GitInspector::business_changes(business_cwd)?;
            info.baseline_file_hashes =
                GitInspector::file_hashes(business_cwd, &info.baseline_changed_files)?;
            info.baseline_cargo_manifest =
                match fs::read_to_string(PathBuf::from(business_cwd).join("Cargo.toml")) {
                    Ok(content) => Some(content),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => {
                        return Err(SddError::new(
                            "E_STATE_CORRUPTED",
                            &format!("读取基线 Cargo.toml 失败：{error}"),
                        ));
                    }
                };
        }
    }

    // 需求获取：续跑时读取 input.md
    let requirement = if continuing && parsed.requirement.is_none() {
        Some(
            fs::read_to_string(run_dir.join("input.md"))
                .map_err(|e| {
                    SddError::new("E_MISSING_ARTIFACT", &format!("读取需求输入失败：{e}"))
                })?
                .trim_end()
                .to_string(),
        )
    } else {
        parsed.requirement.clone()
    };
    let Some(requirement) = requirement.filter(|s| !s.trim().is_empty()) else {
        return Err(SddError::new("E_MISSING_ARTIFACT", "需求内容不能为空"));
    };

    // 写 input.md（首次）
    if !continuing {
        fs::write(run_dir.join("input.md"), &requirement)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入需求输入失败：{e}")))?;
    }

    store.update(|s| {
        s.current_change_id = Some(change_id.clone());
        s.current_run_id = Some(run_id.clone());
        s.current_phase = "NEW_STARTED".to_string();
        s.in_progress_phase = Some("NEW_STARTED".to_string());
        s.last_command = Some("sdd new".to_string());
        s.last_error = None;
        s.suggested_command = Some("sdd new".to_string());
        s.workspace = workspace.clone();
    })?;

    // 语义分析：未回答的 BLOCKER 问题 → 澄清
    let analysis = engine.analyze(&requirement, &parsed.answers);
    let unanswered: Vec<_> = analysis
        .questions
        .iter()
        .filter(|q| q.severity == "BLOCKER" && !parsed.answers.contains_key(&q.id))
        .collect();

    if !unanswered.is_empty() {
        // 写 CLARIFYING 状态的 spec.json（供审计与恢复）
        let spec_json = json!({
            "schemaVersion": SPEC_SCHEMA_VERSION,
            "status": "CLARIFYING",
            "requirement": requirement,
            "questions": unanswered.iter().map(|q| json!({
                "id": q.id, "severity": q.severity, "question": q.question
            })).collect::<Vec<_>>(),
        });
        let spec_json_text = serde_json::to_string_pretty(&spec_json)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化澄清规格失败：{e}")))?;
        fs::write(change_dir.join("spec.json"), spec_json_text)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入澄清规格失败：{e}")))?;
        if parsed.non_interactive {
            return Err(SddError::new(
                "E_UNRESOLVED_BLOCKER",
                "非交互模式下 BLOCKER 问题必须提供答案",
            )
            .with_next("sdd new"));
        }
        store.update(|s| {
            s.current_phase = "CLARIFYING".to_string();
            s.in_progress_phase = None;
            s.suggested_command = Some("sdd new".to_string());
        })?;
        return Ok(CommandResult {
            ok: true,
            state: "CLARIFYING".to_string(),
            exit_code: 0,
            change_id: Some(change_id),
            next: Some("sdd new".to_string()),
            data: Some(json!({
                "clarification": {
                    "questions": unanswered.iter().map(|q| json!({
                        "id": q.id,
                        "question": q.question
                    })).collect::<Vec<_>>()
                }
            })),
            rendered: None,
            warnings: None,
            action_required: None,
            error: None,
        });
    }

    // 需求充分：生成规格
    let codebase_summary = fs::read_to_string(
        PathBuf::from(cwd).join(".sdd/index/codebase-summary.md"),
    )
    .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取代码库摘要失败：{e}")))?;
    let input = GenerateSpecInput {
        requirement: requirement.clone(),
        codebase_summary,
        answers: parsed.answers.clone(),
    };
    let artifacts = engine
        .generate(&input)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("生成规格失败：{e}")))?;

    // 写 spec.md 与 spec.json（status=READY）
    let spec_json = json!({
        "schemaVersion": SPEC_SCHEMA_VERSION,
        "status": "READY",
        "requirement": requirement,
        "proposal": artifacts.proposal,
        "impact": artifacts.impact,
        "questions": artifacts.questions,
        "answers": artifacts.answers,
        "assumptions": artifacts.assumptions,
        "delta": artifacts.delta,
        "model": artifacts.model,
    });
    fs::write(change_dir.join("spec.md"), &artifacts.spec)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 spec.md 失败：{e}")))?;
    let spec_json_text = serde_json::to_string_pretty(&spec_json)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化 spec.json 失败：{e}")))?;
    fs::write(change_dir.join("spec.json"), &spec_json_text)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 spec.json 失败：{e}")))?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:spec"),
        "spec",
        &format!(".sdd/changes/{change_id}/spec.json"),
        &spec_json_text,
        json!({ "requirement": requirement }),
    )?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:spec-md"),
        "spec",
        &format!(".sdd/changes/{change_id}/spec.md"),
        &artifacts.spec,
        json!({ "specJson": crate::policies::digest::digest(&spec_json_text) }),
    )?;

    store.update(|s| {
        s.current_phase = "SPEC_READY".to_string();
        s.in_progress_phase = None;
        s.suggested_command = Some("sdd design".to_string());
        s.last_error = None;
        s.failed_command = None;
        s.interrupted_command = None;
        s.recoverable = true;
    })?;

    Ok(CommandResult {
        ok: true,
        state: "SPEC_READY".to_string(),
        exit_code: 0,
        change_id: Some(change_id),
        next: Some("sdd design".to_string()),
        data: Some(json!({ "spec": artifacts.model })),
        rendered: None,
        warnings: None,
        action_required: None,
        error: None,
    })
}

/// 生成 change id：change-<epoch 纳秒>
pub fn make_change_id() -> String {
    format!("change-{}", epoch_nanos())
}

/// 生成 run id：run-<epoch 纳秒>
pub fn make_run_id() -> String {
    format!("run-{}", epoch_nanos())
}

fn epoch_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

/// 读取当前 change 的 spec（供后续命令复用）
pub fn read_spec_model(
    cwd: &str,
    change_id: &str,
) -> Result<crate::engines::openspec::model::SpecDocument, SddError> {
    let path = PathBuf::from(cwd)
        .join(".sdd/changes")
        .join(change_id)
        .join("spec.json");
    let raw = fs::read_to_string(&path)
        .map_err(|e| SddError::new("E_MISSING_ARTIFACT", &format!("读取 spec.json 失败：{e}")))?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("spec.json 解析失败：{e}")))?;
    let model = value
        .get("model")
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "spec.json 缺少 model 字段"))?;
    serde_json::from_value(model.clone())
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("spec model 解析失败：{e}")))
}

/// 获取当前活动 change id（无则报 E_MISSING_CHANGE）
pub fn current_change_id(state: &WorkflowState) -> Result<String, SddError> {
    let change_id = state.current_change_id.clone().ok_or_else(|| {
        SddError::new("E_MISSING_CHANGE", "当前没有活动变更").with_next("sdd new")
    })?;
    crate::git::isolation::validate_change_id(&change_id)?;
    Ok(change_id)
}
