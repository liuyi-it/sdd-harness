//! codebase doctor / query 子命令诊断测试。
//!
//! 环境未安装 codegraph 时：doctor 返回未安装诊断，query 降级并带 warning。
//! 环境安装了 codegraph 时（CI/开发者机器），跳过降级断言，仅验证结构。

use sdd_core::contracts::CommandRequest;
use sdd_core::knowledge::provider::find_on_path;
use sdd_core::run;
use serde_json::json;

fn prepare(dir: &std::path::Path) -> String {
    std::fs::write(dir.join("README.md"), "# demo").unwrap();
    let cwd = dir.to_string_lossy().to_string();
    run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    cwd
}

#[test]
fn codebase_doctor_returns_provider_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    let result = run(&CommandRequest {
        command: "codebase".into(),
        cwd,
        args: Some(json!({ "sub": "doctor" })),
    })
    .unwrap();
    let data = result.data.expect("doctor 应返回 data");
    let providers = data["providers"].as_array().expect("providers 应为数组");
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0]["provider"], "codegraph");
    if find_on_path("codegraph").is_none() {
        // 环境无 codegraph：诊断明确标记未安装
        assert_eq!(providers[0]["installed"], false);
        assert_eq!(providers[0]["indexed"], false);
    }
}

#[test]
fn codebase_query_degrades_with_warning_without_codegraph() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    let result = run(&CommandRequest {
        command: "codebase".into(),
        cwd,
        args: Some(json!({ "sub": "query", "query": "order_service" })),
    })
    .unwrap();
    assert!(result.ok);
    if find_on_path("codegraph").is_none() {
        // 降级：provider=fallback-file-scan、degraded=true、带 W_KNOWLEDGE_UNAVAILABLE 警告
        let data = result.data.as_ref().unwrap();
        assert_eq!(data["provider"], "fallback-file-scan");
        assert_eq!(data["degraded"], true);
        let warnings = result.warnings.as_ref().expect("降级应带警告");
        assert_eq!(warnings[0]["code"], "W_KNOWLEDGE_UNAVAILABLE");
    }
}

#[test]
fn codebase_doctor_reports_unavailable_index_without_codegraph() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    let result = run(&CommandRequest {
        command: "codebase".into(),
        cwd,
        args: Some(json!({ "sub": "status" })),
    })
    .unwrap();
    assert!(result.ok);
    let data = result.data.unwrap();
    let providers = data["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 1);
    // status 只探测不索引；未安装时 indexed=false
    if find_on_path("codegraph").is_none() {
        assert_eq!(providers[0]["installed"], false);
        assert_eq!(providers[0]["indexed"], false);
    }
}
