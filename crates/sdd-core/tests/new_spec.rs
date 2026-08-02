//! new 命令与 spec engine 测试。

use sdd_core::commands::new::run_new;
use sdd_core::contracts::CommandRequest;
use sdd_core::engines::openspec::model::SpecDocument;
use sdd_core::engines::spec::spec_engine::SpecEngine;
use sdd_core::run;
use serde_json::json;

const FULL_REQUIREMENT: &str = "授权用户通过 API 请求取消待处理订单，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";

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
fn new_without_requirement_returns_missing_artifact() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let err = run_new(
        dir.path().to_string_lossy().as_ref(),
        Some(&json!({ "requirement": null })),
        &SpecEngine::new(),
    )
    .unwrap_err();
    assert_eq!(err.code, "E_MISSING_ARTIFACT");
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
    // spec.json 落盘为 CLARIFYING 状态
    let change_dir = std::fs::read_dir(dir.path().join(".sdd/changes"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let spec_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(change_dir.join("spec.json")).unwrap())
            .unwrap();
    assert_eq!(spec_json.get("status").unwrap(), "CLARIFYING");
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
    assert!(change_dir.join("spec.md").exists());
    let spec_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(change_dir.join("spec.json")).unwrap())
            .unwrap();
    assert_eq!(spec_json.get("status").unwrap(), "READY");
    let model: SpecDocument =
        serde_json::from_value(spec_json.get("model").unwrap().clone()).unwrap();
    assert!(!model.requirements.is_empty());
}

#[test]
fn spec_parse_render_roundtrip() {
    let engine = SpecEngine::new();
    let artifacts = engine
        .generate(&sdd_core::engines::spec::spec_engine::GenerateSpecInput {
            requirement: FULL_REQUIREMENT.to_string(),
            codebase_summary: "".to_string(),
            answers: Default::default(),
        })
        .unwrap();
    let parsed = engine.parse_spec_md(&artifacts.spec).unwrap();
    assert_eq!(
        parsed.requirements.len(),
        artifacts.model.requirements.len()
    );
    assert_eq!(parsed.requirements[0].id, "REQ-001");
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
