//! new 命令与 spec engine 测试。

use sdd_core::commands::new::run_new;
use sdd_core::contracts::CommandRequest;
use sdd_core::engines::spec::spec_engine::SpecEngine;
use sdd_core::engines::spec::SpecDocument;
use sdd_core::run;
use serde_json::json;

const FULL_REQUIREMENT: &str = "授权用户通过 POST /orders/{id}/cancel 请求取消待处理订单，入参 order_id，返回 status 和 error_code，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";

fn spec_value(cwd: &str, change_id: &str) -> serde_json::Value {
    sdd_core::state::RuntimeStore::new(cwd.to_string())
        .read()
        .unwrap()
        .changes[change_id]["spec"]
        .clone()
}

fn init(dir: &std::path::Path) {
    std::fs::write(dir.join("README.md"), "# demo").unwrap();
    let _ = run(&CommandRequest {
        command: "init".into(),
        cwd: dir.to_string_lossy().to_string(),
        args: None,
    })
    .unwrap();
}

fn new_request(dir: &std::path::Path, requirement: &str) -> sdd_core::contracts::CommandResult {
    run_new(
        dir.to_string_lossy().as_ref(),
        Some(&json!({ "requirement": requirement })),
        &SpecEngine::new(),
    )
    .unwrap()
}

#[test]
fn new_without_requirement_returns_invalid_requirement() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let err = run_new(
        dir.path().to_string_lossy().as_ref(),
        None,
        &SpecEngine::new(),
    )
    .unwrap_err();
    assert_eq!(err.code, "E_INVALID_REQUIREMENT");

    let err = run_new(
        dir.path().to_string_lossy().as_ref(),
        Some(&json!({ "requirement": null })),
        &SpecEngine::new(),
    )
    .unwrap_err();
    assert_eq!(err.code, "E_INVALID_PHASE_COMMAND");
}

#[test]
fn new_with_incomplete_requirement_enters_clarifying() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    // 语义槽缺失 → BLOCKER 澄清
    let result = new_request(dir.path(), "实现订单取消功能");
    assert_eq!(result.state, "CLARIFYING");
    let data = result.data.expect("应有澄清数据");
    let questions = data
        .get("clarification")
        .and_then(|c| c.get("questions"))
        .and_then(|q| q.as_array());
    assert!(questions.is_some() && !questions.unwrap().is_empty());
    // 规格模型写入 runtime.json，人工文档同步存在
    let cwd = dir.path().to_string_lossy().to_string();
    let change_id = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap()
        .current_change_id
        .unwrap();
    let spec_json = spec_value(&cwd, &change_id);
    assert_eq!(spec_json.get("status").unwrap(), "CLARIFYING");
    let change_dir = std::fs::read_dir(dir.path().join(".sdd/changes"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(change_dir.join("spec.md").exists());
}

#[test]
fn clarifying_resume_merges_answers_and_original_requirement() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let first = new_request(dir.path(), "实现订单取消功能");
    assert_eq!(first.state, "CLARIFYING");
    let cwd = dir.path().to_string_lossy().to_string();
    let second = run_new(
        &cwd,
        Some(&json!({ "answers": { "Q-ACTOR": "授权用户" } })),
        &SpecEngine::new(),
    )
    .unwrap();
    assert_eq!(second.state, "CLARIFYING");
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let run_id = state.current_run_id.unwrap();
    let runtime = sdd_core::state::RuntimeStore::new(cwd).read().unwrap();
    let input = &runtime.runs[&run_id]["input"];
    let answers = &runtime.runs[&run_id]["answers"];
    assert_eq!(input, "实现订单取消功能");
    assert_eq!(answers["Q-ACTOR"], "授权用户");
}

#[test]
fn new_with_full_requirement_writes_spec() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let result = new_request(dir.path(), FULL_REQUIREMENT);
    assert!(result.ok, "应生成规格");
    assert_eq!(result.state, "SPEC_READY");
    let change_dir = std::fs::read_dir(dir.path().join(".sdd/changes"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let spec_markdown = std::fs::read_to_string(change_dir.join("spec.md")).unwrap();
    assert!(spec_markdown.contains("## 目标与价值"));
    assert!(spec_markdown.contains("## 验收标准"));
    assert!(spec_markdown.contains("### REQ-001："));
    assert!(spec_markdown.contains("- 前提："));
    assert!(!spec_markdown.contains("ADDED Requirements"));
    assert!(!spec_markdown.contains("SHALL"));
    let cwd = dir.path().to_string_lossy().to_string();
    let change_id = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap()
        .current_change_id
        .unwrap();
    let spec_json = spec_value(&cwd, &change_id);
    assert_eq!(spec_json.get("status").unwrap(), "READY");
    assert_eq!(spec_json.get("schemaVersion").unwrap(), "3.0.0");
    assert!(spec_json.get("spec").is_none());
    let model: SpecDocument =
        serde_json::from_value(spec_json.get("model").unwrap().clone()).unwrap();
    assert!(!model.requirements.is_empty());
}

#[test]
fn new_does_not_silently_drop_an_unborn_git_repository() {
    let dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    init(dir.path());

    let error = run_new(
        dir.path().to_string_lossy().as_ref(),
        Some(&json!({ "requirement": FULL_REQUIREMENT })),
        &SpecEngine::new(),
    )
    .unwrap_err();
    assert_eq!(error.code, "E_PATH_OUTSIDE_REPO");
    assert!(std::fs::read_dir(dir.path().join(".sdd/changes"))
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn new_before_init_uses_not_initialized_contract() {
    let dir = tempfile::tempdir().unwrap();
    let error = run_new(
        dir.path().to_string_lossy().as_ref(),
        Some(&json!({ "requirement": FULL_REQUIREMENT })),
        &SpecEngine::new(),
    )
    .unwrap_err();

    assert_eq!(error.code, "E_NOT_INITIALIZED");
    assert_eq!(error.next.as_deref(), Some("sdd init"));
    assert!(!dir.path().join(".sdd").exists());
}

#[test]
fn dispatcher_accepts_explicit_change_id_when_starting_a_change() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let result = run(&CommandRequest {
        command: "new".into(),
        cwd: dir.path().to_string_lossy().to_string(),
        args: Some(json!({
            "changeId": "explicit-change",
            "requirement": FULL_REQUIREMENT
        })),
    })
    .unwrap();

    assert_eq!(result.change_id.as_deref(), Some("explicit-change"));
    assert_eq!(result.state, "SPEC_READY");
}

#[test]
fn generated_model_renders_project_native_acceptance_criteria() {
    let engine = SpecEngine::new();
    let artifacts = engine
        .generate(&sdd_core::engines::spec::spec_engine::GenerateSpecInput {
            requirement: FULL_REQUIREMENT.to_string(),
            codebase_summary: "".to_string(),
            answers: Default::default(),
        })
        .unwrap();
    let rendered = sdd_core::engines::spec::renderer::render_spec(&artifacts.model).unwrap();
    assert_eq!(artifacts.model.requirements[0].id, "REQ-001");
    assert!(rendered.contains("### REQ-001："));
    assert!(rendered.contains("#### REQ-001-SC-001："));
    assert!(artifacts.model.requirements.iter().any(|requirement| {
        requirement.scenarios.iter().any(|scenario| {
            !scenario.given.is_empty() && !scenario.when.is_empty() && !scenario.then.is_empty()
        })
    }));
    assert!(artifacts
        .model
        .requirements
        .iter()
        .all(|requirement| !requirement.statement.contains("SHALL")));
}

#[test]
fn non_interactive_clarifying_fails_with_blocker() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let err = run_new(
        dir.path().to_string_lossy().as_ref(),
        Some(&json!({ "requirement": "实现订单取消功能", "nonInteractive": true })),
        &SpecEngine::new(),
    )
    .unwrap_err();
    assert_eq!(err.code, "E_UNRESOLVED_BLOCKER");
}

#[test]
fn new_rejects_unsafe_change_id_before_creating_change_directory() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let err = run_new(
        dir.path().to_string_lossy().as_ref(),
        Some(&json!({ "requirement": FULL_REQUIREMENT, "changeId": "../escape" })),
        &SpecEngine::new(),
    )
    .unwrap_err();
    assert_eq!(err.code, "E_SECURITY_BLOCKED");
    assert!(!dir.path().join(".sdd/escape").exists());
}

#[test]
fn new_rejects_non_string_answers() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let err = run_new(
        dir.path().to_string_lossy().as_ref(),
        Some(&json!({ "requirement": FULL_REQUIREMENT, "answers": { "Q-001": 1 } })),
        &SpecEngine::new(),
    )
    .unwrap_err();
    assert_eq!(err.code, "E_INVALID_PHASE_COMMAND");
}

#[cfg(unix)]
#[test]
fn clarifying_resume_rejects_symlinked_spec_document() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let first = new_request(dir.path(), "实现订单取消功能");
    assert_eq!(first.state, "CLARIFYING");
    let change_id = first.change_id.unwrap();
    let spec_path = dir
        .path()
        .join(".sdd/changes")
        .join(change_id)
        .join("spec.md");
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), "不得改写").unwrap();
    std::fs::remove_file(&spec_path).unwrap();
    std::os::unix::fs::symlink(outside.path(), &spec_path).unwrap();

    let error = run_new(
        dir.path().to_string_lossy().as_ref(),
        Some(&json!({ "answers": { "Q-ACTOR": "授权用户" } })),
        &SpecEngine::new(),
    )
    .unwrap_err();

    assert_eq!(error.code, "E_SYMLINK_BLOCKED");
    assert_eq!(std::fs::read_to_string(outside.path()).unwrap(), "不得改写");
}

#[test]
fn clarification_uses_frontier_rounds_and_recommendations() {
    let engine = SpecEngine::new();
    let first = engine.analyze("实现订单取消功能", &Default::default());
    assert!(!first.questions.is_empty());
    assert!(first.questions.iter().all(|question| question.round == 1));
    assert!(first
        .questions
        .iter()
        .any(|question| question.id == "Q-GOAL"));
    assert!(first
        .questions
        .iter()
        .all(|question| !question.recommendation.is_empty()));

    let answers = serde_json::from_value::<std::collections::HashMap<String, String>>(json!({
        "Q-GOAL": "取消待处理订单后，订单状态变为已取消",
        "Q-SCOPE": "只改订单服务 POST /orders/{id}/cancel 接口，请求字段 order_id，响应字段 status 和 error_code，不改前端和支付服务",
        "Q-ACCEPTANCE": "覆盖成功取消和重复取消，断言状态与错误码",
    }))
    .unwrap();
    let second = engine.analyze("实现订单取消功能", &answers);
    assert!(second.questions.iter().all(|question| question.round == 2));
    assert!(second
        .questions
        .iter()
        .any(|question| question.id == "Q-ACTOR"));
}

#[test]
fn generated_change_id_uses_requirement_words_and_resolves_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let changes = dir.path().join("changes");
    let first = sdd_core::commands::new::make_change_id("实现订单取消功能", &changes);
    assert_eq!(first, "实现订单取消功能");
    std::fs::create_dir_all(changes.join(&first)).unwrap();
    let second = sdd_core::commands::new::make_change_id("实现订单取消功能", &changes);
    assert_eq!(second, "实现订单取消功能-2");
}

#[test]
fn new_answers_resume_interrupted_new_started_change() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let first = new_request(
        dir.path(),
        "授权用户通过 API 请求取消待处理订单，返回取消成功",
    );
    assert_eq!(first.state, "CLARIFYING");

    let cwd = dir.path().to_string_lossy().to_string();
    sdd_core::state::StateStore::new(cwd.clone())
        .update(|state| {
            state.current_phase = "NEW_STARTED".into();
            state.in_progress_phase = Some("NEW_STARTED".into());
        })
        .unwrap();

    let result = run_new(
        &cwd,
        Some(&json!({
            "answers": {
                "Q-ACTOR": "授权用户",
                "Q-AUTHORIZATION": "授权用户有权限",
                "Q-ACTION": "取消待处理订单",
                "Q-INTERFACE": "POST /orders/{id}/cancel，入参 order_id，返回 status 和 error_code",
                "Q-PRECONDITION": "订单处于待处理状态",
                "Q-RESULT": "返回取消成功",
                "Q-FAILURE": "取消待处理订单返回未授权错误",
                "Q-TEST": "需要自动化测试覆盖成功与未授权"
            }
        })),
        &SpecEngine::new(),
    )
    .unwrap();
    assert_eq!(result.state, "SPEC_READY");
}

#[test]
fn new_in_spec_ready_requires_change_instead_of_overwriting() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let created = new_request(dir.path(), FULL_REQUIREMENT);
    assert_eq!(created.state, "SPEC_READY");
    let cwd = dir.path().to_string_lossy().to_string();
    let change_id = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap()
        .current_change_id
        .unwrap();

    // SPEC_READY 且有活动变更：禁止无提示覆盖，必须走 sdd change。
    let err = run_new(
        &cwd,
        Some(&json!({ "requirement": "另一个需求" })),
        &SpecEngine::new(),
    )
    .unwrap_err();
    assert_eq!(err.code, "E_ACTIVE_CHANGE_EXISTS");
    assert_eq!(err.next, Some(format!("sdd change {change_id}")));
    // spec 未被覆盖
    let spec_json = spec_value(&cwd, &change_id);
    assert_eq!(spec_json.get("requirement").unwrap(), FULL_REQUIREMENT);
}

#[test]
fn new_rejects_requirement_over_32768_chars() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let long_requirement = "需".repeat(32_769);
    let err = run_new(
        dir.path().to_string_lossy().as_ref(),
        Some(&json!({ "requirement": long_requirement })),
        &SpecEngine::new(),
    )
    .unwrap_err();
    assert_eq!(err.code, "E_INVALID_REQUIREMENT");
}

#[cfg(unix)]
#[test]
fn new_rejects_symlinked_changes_directory() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    init(dir.path());
    std::os::unix::fs::symlink(outside.path(), dir.path().join(".sdd/changes")).unwrap();

    let error = run_new(
        dir.path().to_string_lossy().as_ref(),
        Some(&json!({ "requirement": FULL_REQUIREMENT })),
        &SpecEngine::new(),
    )
    .unwrap_err();

    assert_eq!(error.code, "E_SYMLINK_BLOCKED");
    assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());
}
