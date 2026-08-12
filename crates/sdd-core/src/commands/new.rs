//! new 命令：接收粗略需求、提出阻塞问题，并在信息充分后生成首批规格制品。
//!
//! 机器规格存储在 `.sdd/runtime.json`，change 目录只写可读的 spec.md。
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

/// runtime 中规格对象的 schema 版本（与 Node 版一致）。
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
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from),
            answers,
            non_interactive: args
                .get("nonInteractive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }
}

fn can_resume_new(state: &WorkflowState) -> bool {
    state.current_phase == "NEW_STARTED"
        && state.current_change_id.is_some()
        && state.current_run_id.is_some()
}

fn recovery_error(state: &WorkflowState) -> SddError {
    if state.current_phase == "NEW_STARTED" && !can_resume_new(state) {
        SddError::new(
            "E_STATE_CORRUPTED",
            "NEW_STARTED 状态缺少可恢复的当前 change/run",
        )
        .with_next("sdd status")
    } else {
        let code = if state.current_change_id.is_some() {
            "E_ACTIVE_CHANGE_EXISTS"
        } else {
            "E_INVALID_PHASE_COMMAND"
        };
        SddError::new(
            code,
            &format!("无法在 {} 状态下开启新的变更", state.current_phase),
        )
        .with_next(state.suggested_command.as_deref().unwrap_or("sdd status"))
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
    // 阶段前置检查：中断在 NEW_STARTED 的当前 change/run 可安全续跑。
    let continuing = state.current_phase == "CLARIFYING"
        || state.current_phase == "SPEC_READY"
        || state.current_phase == "FAILED"
        || can_resume_new(&state);
    if state.current_phase != "INDEX_READY"
        && state.current_phase != "CLARIFYING"
        && state.current_phase != "ARCHIVED"
        && !continuing
    {
        return Err(recovery_error(&state));
    }
    if state.current_phase == "NOT_INITIALIZED" {
        return Err(
            SddError::new("E_NOT_INITIALIZED", "请先运行 sdd init 再执行其他命令")
                .with_next("sdd init"),
        );
    }

    let run_id = if continuing {
        state.current_run_id.clone().unwrap_or_else(make_run_id)
    } else {
        make_run_id()
    };
    crate::state::state_store::validate_run_id(&run_id)?;

    let requirement = if continuing && parsed.requirement.is_none() {
        Some(
            crate::state::runtime_store::read_run_field(cwd, &run_id, "input")?
                .and_then(|value| value.as_str().map(String::from))
                .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少需求输入"))?
                .trim()
                .to_string(),
        )
    } else {
        parsed.requirement.clone()
    };
    let Some(requirement) = requirement.filter(|s| !s.trim().is_empty()) else {
        return Err(SddError::new("E_MISSING_ARTIFACT", "需求内容不能为空"));
    };

    let change_id = if continuing {
        state.current_change_id.clone().unwrap_or_else(|| {
            make_change_id(&requirement, &PathBuf::from(cwd).join(".sdd/changes"))
        })
    } else {
        parsed.change_id.clone().unwrap_or_else(|| {
            make_change_id(&requirement, &PathBuf::from(cwd).join(".sdd/changes"))
        })
    };
    crate::git::isolation::validate_change_id(&change_id)?;

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

    // 续跑时合并已保存答案，避免每次 `sdd new --answers` 丢失前一轮澄清。
    let mut answers = if continuing {
        crate::state::runtime_store::read_run_field(cwd, &run_id, "answers")?
            .and_then(|value| serde_json::from_value::<HashMap<String, String>>(value).ok())
            .unwrap_or_default()
    } else {
        HashMap::new()
    };
    answers.extend(parsed.answers.clone());

    crate::state::runtime_store::write_run_field(cwd, &run_id, "input", json!(requirement))?;
    crate::state::runtime_store::write_run_field(cwd, &run_id, "answers", json!(answers))?;

    // 需求分析先于状态提交，保证解析失败不会留下不可恢复的 NEW_STARTED。
    let analysis = engine.analyze(&requirement, &answers);
    let unanswered: Vec<_> = analysis
        .questions
        .iter()
        .filter(|q| {
            q.severity == "BLOCKER"
                && !answers
                    .get(&q.id)
                    .is_some_and(|answer| !answer.trim().is_empty())
        })
        .collect();

    store.update(|s| {
        s.current_change_id = Some(change_id.clone());
        s.current_run_id = Some(run_id.clone());
        s.current_phase = "NEW_STARTED".to_string();
        s.in_progress_phase = Some("NEW_STARTED".to_string());
        s.last_command = Some("sdd new".to_string());
        s.last_error = None;
        s.suggested_command = Some("sdd new".to_string());
        s.workspace = workspace.clone();
        if !continuing {
            s.tasks.clear();
            s.artifacts.clear();
            s.pending_agent_task = None;
        }
    })?;

    if !unanswered.is_empty() {
        let spec_markdown = render_clarifying_spec(&requirement, &unanswered);
        let spec_json = json!({
            "schemaVersion": SPEC_SCHEMA_VERSION,
            "status": "CLARIFYING",
            "requirement": requirement,
            "questions": unanswered.iter().map(|q| json!({
                "id": q.id,
                "round": q.round,
                "title": q.title,
                "severity": q.severity,
                "question": q.question,
                "recommendation": q.recommendation
            })).collect::<Vec<_>>(),
        });
        fs::write(change_dir.join("spec.md"), &spec_markdown)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 spec.md 失败：{e}")))?;
        crate::state::runtime_store::write_change_field(cwd, &change_id, "spec", spec_json)?;
        crate::state::artifact_store::record_artifact(
            cwd,
            &format!("{change_id}:spec"),
            "spec",
            &format!(".sdd/changes/{change_id}/spec.md"),
            &spec_markdown,
            json!({ "requirement": requirement }),
        )?;
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
                        "round": q.round,
                        "title": q.title,
                        "question": q.question,
                        "recommendation": q.recommendation
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
    let codebase_summary = crate::state::runtime_store::read_index_field(cwd, "summary")?
        .and_then(|value| value.as_str().map(String::from))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少代码库摘要"))?;
    let input = GenerateSpecInput {
        requirement: requirement.clone(),
        codebase_summary,
        answers: answers.clone(),
    };
    let artifacts = engine
        .generate(&input)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("生成规格失败：{e}")))?;

    let spec_markdown = render_spec_document(&requirement, &answers, &artifacts.spec);
    let spec_json = json!({
        "schemaVersion": SPEC_SCHEMA_VERSION,
        "status": "READY",
        "requirement": requirement,
        "impact": artifacts.impact,
        "answers": answers,
        "model": artifacts.model,
    });
    fs::write(change_dir.join("spec.md"), &spec_markdown)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 spec.md 失败：{e}")))?;
    crate::state::runtime_store::write_change_field(cwd, &change_id, "spec", spec_json)?;
    crate::state::artifact_store::record_artifact(
        cwd,
        &format!("{change_id}:spec"),
        "spec",
        &format!(".sdd/changes/{change_id}/spec.md"),
        &spec_markdown,
        json!({ "requirement": requirement }),
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

/// 根据需求生成可读的 change id；同名需求追加序号，不使用时间戳污染名称。
pub fn make_change_id(requirement: &str, changes_dir: &std::path::Path) -> String {
    let base = slugify(requirement);
    let mut candidate = base.clone();
    let mut suffix = 2;
    while changes_dir.join(&candidate).exists() {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

fn slugify(requirement: &str) -> String {
    let mut words = Vec::new();
    let mut word = String::new();
    for character in requirement.chars() {
        if character.is_alphanumeric() {
            word.push(if character.is_ascii_uppercase() {
                character.to_ascii_lowercase()
            } else {
                character
            });
        } else if !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    let mut slug = words.join("-");
    if slug.chars().count() > 64 {
        let prefix = slug.chars().take(64).collect::<String>();
        slug = prefix
            .rsplit_once('-')
            .map_or(prefix.clone(), |(head, _)| head.to_string());
    }
    if slug.is_empty() {
        "change".to_string()
    } else {
        slug
    }
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

/// 读取当前 change 的 spec model（机器数据位于 runtime.json）。
pub fn read_spec_model(
    cwd: &str,
    change_id: &str,
) -> Result<crate::engines::openspec::model::SpecDocument, SddError> {
    let value = crate::state::runtime_store::read_change_field(cwd, change_id, "spec")?
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少 spec"))?;
    let model = value
        .get("model")
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "spec 缺少 model 字段"))?;
    serde_json::from_value(model.clone())
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("spec model 解析失败：{e}")))
}

/// 获取当前活动 change id（无则报 E_MISSING_CHANGE）。
pub fn current_change_id(state: &WorkflowState) -> Result<String, SddError> {
    let change_id = state.current_change_id.clone().ok_or_else(|| {
        SddError::new("E_MISSING_CHANGE", "当前没有活动变更").with_next("sdd new")
    })?;
    crate::git::isolation::validate_change_id(&change_id)?;
    Ok(change_id)
}

fn render_clarifying_spec(
    requirement: &str,
    questions: &[&crate::engines::spec::spec_engine::ClarifyingQuestion],
) -> String {
    let mut lines = vec![
        "# 需求规格".to_string(),
        String::new(),
        "## 当前需求".to_string(),
        String::new(),
        requirement.to_string(),
        String::new(),
        "## 待澄清问题".to_string(),
        String::new(),
    ];
    if let Some(round) = questions.first().map(|question| question.round) {
        lines.extend([format!("### 第 {round} 轮前沿"), String::new()]);
    }
    for question in questions {
        lines.push(format!(
            "- [ ] {}｜{}：{}\n  - 建议：{}",
            question.id, question.title, question.question, question.recommendation
        ));
    }
    lines.join("\n") + "\n"
}

pub(crate) fn render_spec_document(
    requirement: &str,
    answers: &HashMap<String, String>,
    structured_spec: &str,
) -> String {
    let structured_body = structured_spec
        .lines()
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n");
    let mut lines = vec![
        "# 需求规格".to_string(),
        String::new(),
        "## 目标与价值".to_string(),
        String::new(),
        requirement.to_string(),
        String::new(),
        "## 范围".to_string(),
        String::new(),
        "- 包含：需求正文及其验收场景明确的用户行为。".to_string(),
        "- 不包含：未在需求、约束和验收标准中明确的功能、重构或依赖。".to_string(),
        String::new(),
        "## 关键设计决策".to_string(),
        String::new(),
        "- 以需求和场景作为验收边界，具体技术实现放在 plan.md 中审核。".to_string(),
        "- 保持未明确变更的既有行为和对外契约。".to_string(),
        String::new(),
        "## 约束".to_string(),
        String::new(),
        "- 变更必须覆盖安全边界、必要审计和自动化测试。".to_string(),
    ];
    if !answers.is_empty() {
        lines.push(String::new());
        lines.push("### 已确认约束".to_string());
        let mut answer_ids: Vec<&String> = answers.keys().collect();
        answer_ids.sort();
        for id in answer_ids {
            lines.push(format!("- {}：{}", id, answers[id]));
        }
    }
    lines.extend([
        String::new(),
        "## 验收标准".to_string(),
        String::new(),
        structured_body,
        String::new(),
    ]);
    lines.join("\n")
}
