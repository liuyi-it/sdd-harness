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
fn gitnexus_probe_reports_shape_without_panic() {
    let provider = GitNexusProvider::default();
    let result = provider.probe();
    assert!(result.available || result.message.is_some());
}

#[test]
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
    let diags = router.initialize(&cwd);
    assert_eq!(diags.len(), 2); // gitnexus + codegraph 各一条
                                // 诊断文件已写入
    assert!(dir.path().join(".sdd/index/knowledge.json").exists());
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
