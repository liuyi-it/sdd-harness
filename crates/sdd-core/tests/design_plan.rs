//! design 与 plan 命令测试（TDD 任务链）。

use sdd_core::commands::new::run_new;
use sdd_core::contracts::CommandRequest;
use sdd_core::engines::spec::spec_engine::SpecEngine;
use sdd_core::engines::tdd::TddEngine;
use sdd_core::run;
use serde_json::json;

const FULL_REQUIREMENT: &str = "授权用户通过 API 请求取消待处理订单，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";

fn prepare(dir: &std::path::Path) -> String {
    // 准备：init + new（含 index 摘要与 impact 中的文件路径）
    std::fs::write(dir.join("README.md"), "# demo").unwrap();
    let cwd = dir.to_string_lossy().to_string();
    run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    // 写入 index 摘要（含源码与测试文件路径，供 planner 推导范围）
    std::fs::create_dir_all(dir.join(".sdd/index")).unwrap();
    std::fs::write(
        dir.join(".sdd/index/summary.md"),
        "src/order_service.rs\nsrc/order_service.test.rs\nCargo.toml\n",
    )
    .unwrap();
    let result = run_new(
        &cwd,
        Some(&json!({ "requirement": FULL_REQUIREMENT })),
        &SpecEngine::new(),
    )
    .unwrap();
    assert!(result.ok, "new 应成功: {:?}", result.error);
    cwd
}

#[test]
fn tdd_plan_has_red_green_refactor_verify() {
    let engine = TddEngine::new();
    let spec = SpecEngine::new()
        .generate(&sdd_core::engines::spec::spec_engine::GenerateSpecInput {
            requirement: FULL_REQUIREMENT.to_string(),
            codebase_summary: "src/order_service.rs\nsrc/order_service.test.rs\nCargo.toml\n"
                .to_string(),
            answers: Default::default(),
        })
        .unwrap();
    let artifacts = engine
        .generate_plan(&sdd_core::engines::tdd::PlanningInputRust {
            spec: spec.spec,
            design: "# Design\n\n## Target Design\n\norder cancellation".to_string(),
            impact: spec.impact,
            codebase_summary: "src/order_service.rs\nsrc/order_service.test.rs\nCargo.toml\n"
                .to_string(),
        })
        .unwrap();
    let phases: Vec<&str> = artifacts.tasks.iter().map(|t| t.phase.as_str()).collect();
    assert!(phases.contains(&"RED"));
    assert!(phases.contains(&"GREEN"));
    assert!(phases.contains(&"REFACTOR"));
    assert!(phases.contains(&"VERIFY"));
    // 任务 id 格式 TASK-001-RED
    assert!(artifacts.tasks.iter().all(|t| t.id.starts_with("TASK-")));
}

#[test]
fn design_then_plan_updates_phases() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    let design = run(&CommandRequest {
        command: "design".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert!(design.ok, "design 应成功: {:?}", design.error);
    assert_eq!(design.state, "DESIGN_READY");
    assert!(!find_change_dir(dir.path()).join("design.md").exists());
    assert!(find_change_dir(dir.path()).join("spec.md").exists());
    let spec_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(find_change_dir(dir.path()).join("spec.json")).unwrap(),
    )
    .unwrap();
    assert!(spec_json.get("design").and_then(|v| v.as_str()).is_some());

    let plan = run(&CommandRequest {
        command: "plan".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert!(plan.ok, "plan 应成功: {:?}", plan.error);
    assert_eq!(plan.state, "PLAN_READY");
    let change_dir = find_change_dir(dir.path());
    assert!(change_dir.join("plan.json").exists());
    let plan_markdown = std::fs::read_to_string(change_dir.join("plan.md")).unwrap();
    assert!(plan_markdown.contains("## 技术方案与架构"));
    let tasks_markdown = std::fs::read_to_string(change_dir.join("tasks.md")).unwrap();
    assert!(tasks_markdown.contains("# 开发任务"));
    assert!(tasks_markdown.contains("## [ ] TASK-001-RED"));
    let plan_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(change_dir.join("plan.json")).unwrap())
            .unwrap();
    let tasks = plan_json.get("tasks").and_then(|t| t.as_array());
    assert!(tasks.is_some() && !tasks.unwrap().is_empty());
}

fn find_change_dir(root: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(root.join(".sdd/changes"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
}

#[test]
fn plan_requires_source_and_test_files() {
    // 无文件范围信息时 plan 应报 E_UNRESOLVED_BLOCKER
    let engine = TddEngine::new();
    let spec = SpecEngine::new()
        .generate(&sdd_core::engines::spec::spec_engine::GenerateSpecInput {
            requirement: FULL_REQUIREMENT.to_string(),
            codebase_summary: "（无文件信息）".to_string(),
            answers: Default::default(),
        })
        .unwrap();
    let result = engine.generate_plan(&sdd_core::engines::tdd::PlanningInputRust {
        spec: spec.spec,
        design: "# Design".to_string(),
        impact: spec.impact,
        codebase_summary: "（无文件信息）".to_string(),
    });
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.code == "E_UNRESOLVED_BLOCKER");
}

#[test]
fn plan_persists_valid_dependency_decisions() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    run(&CommandRequest {
        command: "design".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    run(&CommandRequest {
        command: "plan".into(),
        cwd,
        args: Some(json!({
            "dependencies": [{
                "name": "serde", "manifest": "Cargo.toml", "action": "ADD",
                "reason": "序列化协议", "requirements": ["REQ-001"]
            }]
        })),
    })
    .unwrap();
    let plan: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(find_change_dir(dir.path()).join("plan.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(plan["dependencies"][0]["name"], "serde");
}

#[test]
fn workflow_command_rejects_non_active_change() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    let error = run(&CommandRequest {
        command: "design".into(),
        cwd,
        args: Some(json!({ "changeId": "another-change" })),
    })
    .unwrap_err();
    assert_eq!(error.code, "E_MISSING_CHANGE");
}
