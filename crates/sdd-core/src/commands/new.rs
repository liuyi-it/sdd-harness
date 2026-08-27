//! new 命令：接收粗略需求、提出阻塞问题，并在信息充分后生成首批规格制品。
//!
//! 机器规格存储在 `.sdd/runtime.json`，change 目录只写可读的 spec.md。
use std::collections::HashMap;
use std::fs;

use serde_json::json;

use crate::contracts::CommandResult;
use crate::engines::spec::spec_engine::{GenerateSpecInput, SpecEngine};
use crate::error::SddError;
use crate::git::GitInspector;
use crate::state::artifact_store::ArtifactRecord;
use crate::state::file_lock::lock_initialized_sdd;
use crate::state::WorkflowState;

/// runtime 中规格对象的当前 schema 版本。
const SPEC_SCHEMA_VERSION: &str = "2.0.0";
const MAX_REQUIREMENT_CHARS: usize = 32_768;

pub(crate) fn validate_requirement_length(requirement: &str) -> Result<(), SddError> {
    if requirement.chars().count() > MAX_REQUIREMENT_CHARS {
        return Err(SddError::new(
            "E_INVALID_REQUIREMENT",
            &format!("需求文本超过 {MAX_REQUIREMENT_CHARS} 字符上限"),
        ));
    }
    Ok(())
}

pub struct NewArgs {
    pub requirement: Option<String>,
    pub change_id: Option<String>,
    pub answers: HashMap<String, String>,
    pub non_interactive: bool,
}

impl NewArgs {
    fn from_json(args: Option<&serde_json::Value>) -> Result<Self, SddError> {
        let empty = serde_json::Value::Null;
        let args = args.unwrap_or(&empty);
        super::validate_string_map_arg(Some(args), "answers")?;
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
            requirement: super::string_arg(Some(args), "requirement")?
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            change_id: super::string_arg(Some(args), "changeId")?
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from),
            answers,
            non_interactive: super::bool_arg(Some(args), "nonInteractive")?.unwrap_or(false),
        })
    }
}

fn recovery_error(state: &WorkflowState) -> SddError {
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

pub fn run_new(
    cwd: &str,
    args: Option<&serde_json::Value>,
    engine: &SpecEngine,
) -> Result<CommandResult, SddError> {
    super::validate_args(
        args,
        &[
            "timeout",
            "changeId",
            "nonInteractive",
            "requirement",
            "answers",
        ],
    )?;
    let parsed = NewArgs::from_json(args)?;
    let timeout_ms = super::timeout_ms(args)?;
    let _guard = lock_initialized_sdd(cwd, "sdd new", parsed.change_id.as_deref(), timeout_ms)?;

    let runtime = crate::state::RuntimeStore::new(cwd.to_string()).read()?;
    let state = runtime.state.clone();
    super::ensure_phase(cwd, &state, "new", args)?;

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
    // SPEC_READY：需求已充分且存在当前变更时必须用 change 修订，禁止无提示覆盖当前 spec。
    if state.current_phase == "SPEC_READY" {
        if let Some(change_id) = state.current_change_id.as_deref() {
            return Err(SddError::new(
                "E_ACTIVE_CHANGE_EXISTS",
                "存在已完成规格的当前变更，请用 sdd change 修订需求，禁止直接覆盖",
            )
            .with_next(&format!("sdd change {change_id}")));
        }
        return Err(recovery_error(&state));
    }
    // 阶段前置检查：中断在 NEW_STARTED 的当前 change/run 可安全续跑；
    // PAUSED/FAILED 由 auto --resume 先走 new 步骤恢复。
    let continuing = state.current_phase == "CLARIFYING"
        || state.current_phase == "PAUSED"
        || state.current_phase == "NEW_STARTED";
    if state.current_phase == "NOT_INITIALIZED" {
        return Err(
            SddError::new("E_NOT_INITIALIZED", "请先运行 sdd init 再执行其他命令")
                .with_next("sdd init"),
        );
    }
    if state.current_phase != "INDEX_READY"
        && state.current_phase != "CLARIFYING"
        && state.current_phase != "ARCHIVED"
        && !continuing
    {
        return Err(recovery_error(&state));
    }

    let run_id = if continuing {
        state
            .current_run_id
            .clone()
            .expect("状态不变量保证恢复阶段存在 currentRunId")
    } else {
        crate::state::state_store::unique_id("run")?
    };
    crate::state::state_store::validate_run_id(&run_id)?;

    let requirement = if continuing && parsed.requirement.is_none() {
        Some(
            runtime
                .runs
                .get(&run_id)
                .and_then(|run| run.get("input"))
                .and_then(|value| value.as_str().map(String::from))
                .expect("runtime 不变量保证 run.input 存在")
                .trim()
                .to_string(),
        )
    } else {
        parsed.requirement.clone()
    };
    let Some(requirement) = requirement.filter(|s| !s.trim().is_empty()) else {
        return Err(SddError::new("E_INVALID_REQUIREMENT", "请提供非空需求文本"));
    };
    validate_requirement_length(&requirement)?;

    let changes_dir = crate::state::paths::changes_dir(cwd, true)?;
    let change_id = if continuing {
        state
            .current_change_id
            .clone()
            .expect("状态不变量保证恢复阶段存在 currentChangeId")
    } else {
        parsed
            .change_id
            .clone()
            .unwrap_or_else(|| make_change_id(&requirement, &changes_dir))
    };
    crate::git::isolation::validate_change_id(&change_id)?;
    if !continuing && runtime.changes.contains_key(&change_id) {
        return Err(SddError::new(
            "E_ACTIVE_CHANGE_EXISTS",
            &format!("变更标识已存在：{change_id}"),
        ));
    }

    match crate::state::paths::change_dir(cwd, &change_id, false) {
        Ok(existing) if !continuing => {
            let mut entries = fs::read_dir(&existing).map_err(|error| {
                SddError::new("E_STATE_CORRUPTED", &format!("读取变更目录失败：{error}"))
            })?;
            if entries
                .next()
                .transpose()
                .map_err(|error| {
                    SddError::new(
                        "E_STATE_CORRUPTED",
                        &format!("读取变更目录条目失败：{error}"),
                    )
                })?
                .is_some()
            {
                return Err(SddError::new(
                    "E_ACTIVE_CHANGE_EXISTS",
                    &format!("变更目录已存在且非空：{change_id}"),
                ));
            }
        }
        Ok(_) => {}
        Err(error) if error.code == "E_MISSING_CHANGE" => {}
        Err(error) => return Err(error),
    }

    let mut workspace = if continuing {
        state.workspace
    } else if crate::git::GitIsolationManager::enabled(&runtime.config)? {
        let handle = crate::git::GitIsolationManager::ensure_worktree(cwd, &change_id)?;
        Some(crate::state::state_store::WorkspaceInfo {
            branch_name: Some(handle.branch),
            worktree_path: Some(handle.worktree_path),
            baseline_commit: handle.baseline_commit,
            ..Default::default()
        })
    } else if GitInspector::is_git_repo(cwd)? {
        Some(crate::state::state_store::WorkspaceInfo {
            branch_name: None,
            worktree_path: None,
            baseline_commit: GitInspector::head(cwd)?,
            ..Default::default()
        })
    } else {
        None
    };
    if !continuing {
        if let Some(info) = workspace.as_mut() {
            let business_cwd = info.worktree_path.as_deref().unwrap_or(cwd);
            info.baseline_changed_files = GitInspector::business_changes(business_cwd)?;
            info.baseline_file_hashes =
                GitInspector::file_hashes(business_cwd, &info.baseline_changed_files)?;
            let cargo_manifest = GitInspector::resolve_repo_path(business_cwd, "Cargo.toml")?;
            info.baseline_cargo_manifest = match fs::read_to_string(cargo_manifest) {
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
        match runtime
            .runs
            .get(&run_id)
            .and_then(|run| run.get("answers"))
            .cloned()
        {
            Some(value) => {
                serde_json::from_value::<HashMap<String, String>>(value).map_err(|error| {
                    SddError::new(
                        "E_STATE_CORRUPTED",
                        &format!("runtime.json 的 answers 结构无效：{error}"),
                    )
                })?
            }
            None => HashMap::new(),
        }
    } else {
        HashMap::new()
    };
    answers.extend(parsed.answers.clone());

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

    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        let run = if continuing {
            document
                .runs
                .get_mut(&run_id)
                .and_then(serde_json::Value::as_object_mut)
                .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "恢复的 run 必须是对象"))?
        } else {
            if document.runs.contains_key(&run_id) {
                return Err(SddError::new("E_STATE_CORRUPTED", "新 runId 发生冲突"));
            }
            document.runs.insert(run_id.clone(), json!({}));
            document
                .runs
                .get_mut(&run_id)
                .and_then(serde_json::Value::as_object_mut)
                .expect("刚插入的 run 必须是对象")
        };
        run.insert("changeId".to_string(), json!(change_id));
        run.insert("input".to_string(), json!(requirement));
        run.insert("answers".to_string(), json!(answers));
        if continuing {
            document
                .changes
                .get(&change_id)
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "恢复的 change 必须是对象"))?;
        } else {
            document.changes.insert(change_id.clone(), json!({}));
        }
        crate::state::state_store::apply_state_update(&mut document.state, |state| {
            state.current_change_id = Some(change_id.clone());
            state.current_run_id = Some(run_id.clone());
            state.current_phase = "NEW_STARTED".to_string();
            state.in_progress_phase = Some("NEW_STARTED".to_string());
            state.last_command = Some("sdd new".to_string());
            state.suggested_command = Some("sdd new".to_string());
            state.workspace = workspace.clone();
            state.clear_failure();
            if !continuing {
                state.tasks.clear();
                state.pending_agent_task = None;
            }
        })?;
        Ok(())
    })?;
    // 状态已记录 NEW_STARTED 后再创建文档目录；前置 Git/需求分析失败不会留下
    // 占用 change ID 的空目录，目录创建失败则可由同一活动变更安全重试。
    let change_dir = crate::state::paths::change_dir(cwd, &change_id, true)?;

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
        crate::safe_fs::atomic_write(
            &change_dir.join("spec.md"),
            spec_markdown.as_bytes(),
            "spec.md",
        )?;
        let artifact_key = format!("{change_id}:spec");
        let content_path = format!(".sdd/changes/{change_id}/spec.md");
        crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
            let change = document
                .changes
                .get_mut(&change_id)
                .and_then(serde_json::Value::as_object_mut)
                .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "当前变更必须是对象"))?;
            change.insert("spec".to_string(), spec_json);
            crate::state::artifact_store::record_artifacts_in(
                cwd,
                document,
                vec![ArtifactRecord {
                    key: &artifact_key,
                    artifact_type: "spec",
                    content_path: &content_path,
                    inputs: json!({ "requirement": &requirement }),
                }],
            )?;
            crate::state::state_store::apply_state_update(&mut document.state, |state| {
                state.current_phase = "CLARIFYING".to_string();
                state.in_progress_phase = None;
                state.clear_failure();
                state.suggested_command = Some("sdd new".to_string());
            })?;
            Ok(())
        })?;
        if parsed.non_interactive {
            return Err(SddError::new(
                "E_UNRESOLVED_BLOCKER",
                "非交互模式下 BLOCKER 问题必须提供答案",
            )
            .with_next("sdd new"));
        }
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
    let codebase_summary = runtime
        .index
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .map(String::from)
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "runtime.json 缺少代码库摘要"))?;
    let input = GenerateSpecInput {
        requirement: requirement.clone(),
        codebase_summary,
        answers: answers.clone(),
    };
    let artifacts = engine
        .generate(&input)
        .map_err(|e| SddError::new("E_GENERATION_FAILED", &format!("生成规格失败：{e}")))?;

    let spec_markdown = render_spec_document(&requirement, &answers, &artifacts.spec);
    let spec_json = json!({
        "schemaVersion": SPEC_SCHEMA_VERSION,
        "status": "READY",
        "requirement": requirement,
        "impact": artifacts.impact,
        "answers": answers,
        "model": artifacts.model,
    });
    crate::safe_fs::atomic_write(
        &change_dir.join("spec.md"),
        spec_markdown.as_bytes(),
        "spec.md",
    )?;
    let artifact_key = format!("{change_id}:spec");
    let content_path = format!(".sdd/changes/{change_id}/spec.md");
    crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
        let change = document
            .changes
            .get_mut(&change_id)
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "当前变更必须是对象"))?;
        change.insert("spec".to_string(), spec_json);
        crate::state::artifact_store::record_artifacts_in(
            cwd,
            document,
            vec![ArtifactRecord {
                key: &artifact_key,
                artifact_type: "spec",
                content_path: &content_path,
                inputs: json!({ "requirement": &requirement }),
            }],
        )?;
        crate::state::state_store::apply_state_update(&mut document.state, |state| {
            state.current_phase = "SPEC_READY".to_string();
            state.in_progress_phase = None;
            state.suggested_command = Some("sdd design".to_string());
            state.clear_failure();
        })?;
        Ok(())
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

/// 获取当前活动 change id（无则报 E_MISSING_CHANGE）。
pub(crate) fn current_change_id(state: &WorkflowState) -> Result<String, SddError> {
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
        crate::engines::openspec::escape_spec_text(requirement),
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
        crate::engines::openspec::escape_spec_text(requirement),
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
            // 答案同样转义结构行，避免澄清回答注入规格结构。
            lines.push(format!(
                "- {}：{}",
                id,
                crate::engines::openspec::escape_spec_text(&answers[id])
            ));
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
