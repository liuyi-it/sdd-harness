//! 知识图谱 Provider 与路由测试。

use sdd_core::knowledge::codegraph::CodeGraphProvider;
use sdd_core::knowledge::gitnexus::GitNexusProvider;
use sdd_core::knowledge::provider::{find_on_path, KnowledgeIntent, KnowledgeProvider};
use sdd_core::knowledge::router::KnowledgeRouter;

#[test]
fn find_on_path_locates_git() {
    let found = find_on_path("git").expect("git 应可探测到");
    assert!(found.exists());
}

#[test]
#[ignore = "可选真实 CLI 探测；确定性行为由 fake provider 测试覆盖"]
fn gitnexus_probe_reports_shape_without_panic() {
    let provider = GitNexusProvider::default();
    let result = provider.probe();
    assert!(result.available || result.message.is_some());
}

#[test]
#[ignore = "可选真实 CLI 探测；确定性行为由 fake provider 测试覆盖"]
fn codegraph_probe_same_shape() {
    let provider = CodeGraphProvider::default();
    let result = provider.probe();
    assert!(result.available || result.message.is_some());
}

#[test]
fn query_when_unavailable_is_degraded() {
    let provider = GitNexusProvider::default();
    if !provider.probe().available {
        let result = provider.query(".", KnowledgeIntent::Impact, "foo");
        assert!(result.degraded);
        assert!(result.reason.is_some());
        assert!(result.confidence <= 0.45);
    }
}

#[test]
fn intent_roundtrip() {
    for name in [
        "impact",
        "context",
        "explore",
        "callers",
        "callees",
        "related-files",
        "tests",
        "routes",
        "architecture",
    ] {
        let intent = KnowledgeIntent::parse(name).expect("intent 应可解析");
        assert_eq!(intent.as_str(), name);
    }
    assert!(KnowledgeIntent::parse("nonsense").is_none());
}

#[test]
fn initialize_writes_diagnostics_without_failing() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let router = KnowledgeRouter::new();
    let diags = router.initialize(&cwd, 600_000).unwrap();
    assert_eq!(diags.len(), 2); // gitnexus + codegraph 各一条
                                // 诊断摘要已写入 runtime.json
    let runtime: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".sdd/runtime.json")).unwrap(),
    )
    .unwrap();
    assert!(runtime["index"]["diagnostics"].is_array());
    assert!(runtime["index"]["summary"].is_string());
}

#[test]
fn query_returns_known_shape() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let router = KnowledgeRouter::new();
    let result = router.query(&cwd, KnowledgeIntent::Impact, "main");
    assert!(result.confidence <= 0.99);
    assert!(result.confidence >= 0.0);
    // provider 必须是三个合法值之一
    assert!(
        ["gitnexus", "codegraph", "fallback-file-scan"].contains(&result.provider),
        "未知 provider: {}",
        result.provider
    );
}

#[test]
fn fallback_scan_never_panics_and_is_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let result = KnowledgeRouter::fallback_scan(&cwd, KnowledgeIntent::Architecture, "");
    assert!(result.degraded);
    assert_eq!(result.provider, "fallback-file-scan");
    assert!(result.payload.get("codebaseSummary").is_some());
}

#[test]
fn fallback_scan_skips_secret_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("normal.txt"), "hello").unwrap();
    std::fs::write(dir.path().join("id_rsa"), "secret").unwrap();
    std::fs::write(dir.path().join("secret.pem"), "secret").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let result = KnowledgeRouter::fallback_scan(&cwd, KnowledgeIntent::Architecture, "");
    let summary = result
        .payload
        .get("codebaseSummary")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(summary.contains("normal.txt"));
    assert!(!summary.contains("id_rsa"));
    assert!(!summary.contains("secret.pem"));
}

#[cfg(unix)]
fn fake_cli() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fake-cli");
    std::fs::write(&path, "#!/bin/sh\nprintf '%s' \"$*\"\n").unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
    dir
}

#[cfg(unix)]
#[test]
fn fake_providers_use_documented_query_commands() {
    let fake = fake_cli();
    let bin = fake.path().join("fake-cli");
    let codegraph = CodeGraphProvider {
        bin: Some(bin.clone()),
    };
    let gitnexus = GitNexusProvider { bin: Some(bin) };
    let root = fake.path().to_string_lossy();

    let codegraph_result = codegraph.query(&root, KnowledgeIntent::Context, "OrderService");
    assert_eq!(
        codegraph_result.payload["output"],
        format!("explore OrderService --path {root}")
    );
    let gitnexus_result = gitnexus.query(&root, KnowledgeIntent::Impact, "cancel_order");
    assert_eq!(
        gitnexus_result.payload["output"],
        "impact --summary-only cancel_order"
    );
}
