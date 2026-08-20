//! 知识图谱 Provider 与路由测试。

use sdd_core::knowledge::codegraph::CodeGraphProvider;
use sdd_core::knowledge::provider::{find_on_path, KnowledgeIntent, KnowledgeProvider};
use sdd_core::knowledge::router::KnowledgeRouter;

#[test]
fn find_on_path_locates_git() {
    let found = find_on_path("git").expect("git 应可探测到");
    assert!(found.exists());
}

#[test]
#[ignore = "可选真实 CLI 探测；确定性行为由 fake provider 测试覆盖"]
fn codegraph_probe_reports_shape_without_panic() {
    let provider = CodeGraphProvider::default();
    let result = provider.probe();
    assert!(result.available || result.message.is_some());
}

#[test]
fn query_when_unavailable_is_degraded() {
    let provider = CodeGraphProvider::default();
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
    assert_eq!(diags.len(), 1); // CodeGraph 一条诊断
                                // 诊断摘要已写入 runtime.json
    let runtime: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".sdd/runtime.json")).unwrap(),
    )
    .unwrap();
    assert!(runtime["index"]["diagnostics"].is_array());
    let summary = runtime["index"]["summary"]
        .as_str()
        .expect("summary 应为字符串");
    // 摘要首行必须是来源 meta 注释（双轨化）
    assert!(
        summary.starts_with("<!-- summary-provider:"),
        "summary 首行应为 meta 注释，实际: {summary:?}"
    );
}

#[test]
fn query_returns_known_shape() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let router = KnowledgeRouter::new();
    let result = router.query(&cwd, KnowledgeIntent::Impact, "main");
    assert!(result.confidence <= 0.99);
    assert!(result.confidence >= 0.0);
    // provider 必须是 CodeGraph 或受限文件扫描
    assert!(
        ["codegraph", "fallback-file-scan"].contains(&result.provider),
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

/// 构造记录每次调用参数的 fake-cli（供探测缓存计数）
#[cfg(unix)]
fn fake_cli_logging(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let log = dir.join("calls.log");
    let path = dir.join("fake-cli");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nprintf '%s' \"$*\"\n",
        log.display()
    );
    std::fs::write(&path, script).unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn fake_codegraph_uses_documented_query_commands() {
    let fake = fake_cli();
    let bin = fake.path().join("fake-cli");
    let codegraph = CodeGraphProvider {
        bin: Some(bin.clone()),
    };
    let root = fake.path().to_string_lossy();

    let codegraph_result = codegraph.query(&root, KnowledgeIntent::Context, "OrderService");
    assert_eq!(
        codegraph_result.payload["output"],
        format!("explore OrderService --path {root}")
    );
    let impact_result = codegraph.query(&root, KnowledgeIntent::Impact, "cancel_order");
    assert_eq!(
        impact_result.payload["output"],
        format!("impact cancel_order --path {root}")
    );
}

#[cfg(unix)]
#[test]
fn router_uses_codegraph_for_every_intent() {
    let fake = fake_cli();
    let bin = fake.path().join("fake-cli");
    let root = fake.path().to_string_lossy();
    let router = KnowledgeRouter {
        codegraph: CodeGraphProvider { bin: Some(bin) },
    };

    let result = router.query(&root, KnowledgeIntent::Impact, "cancel_order");
    assert!(!result.degraded);
    assert_eq!(
        result.payload["output"],
        format!("impact cancel_order --path {root}")
    );
}

#[cfg(unix)]
#[test]
fn query_caches_probe_result_within_ttl() {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_cli_logging(dir.path());
    let root = dir.path().to_string_lossy();
    let router = KnowledgeRouter {
        codegraph: CodeGraphProvider { bin: Some(bin) },
    };

    let _ = router.query(&root, KnowledgeIntent::Impact, "cancel_order");
    let _ = router.query(&root, KnowledgeIntent::Impact, "cancel_order");

    let calls = std::fs::read_to_string(dir.path().join("calls.log")).unwrap();
    let probe_count = calls.lines().filter(|l| l.starts_with("--version")).count();
    assert_eq!(probe_count, 1, "两次查询只应探测一次 codegraph（TTL 缓存）");
    assert_eq!(
        calls.lines().filter(|l| l.starts_with("impact")).count(),
        2,
        "两次查询命令都应执行"
    );
}

#[cfg(unix)]
#[test]
fn initialize_uses_codegraph_summary_when_available() {
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_cli_logging(dir.path());
    let root = dir.path().to_string_lossy();
    let router = KnowledgeRouter {
        codegraph: CodeGraphProvider { bin: Some(bin) },
    };

    router.initialize(&root, 600_000).unwrap();

    let runtime: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".sdd/runtime.json")).unwrap(),
    )
    .unwrap();
    let summary = runtime["index"]["summary"]
        .as_str()
        .expect("summary 应为字符串");
    // CodeGraph 可用且索引成功：summary 首行标记 codegraph 来源，正文为查询输出
    assert!(
        summary.starts_with("<!-- summary-provider: codegraph degraded=false -->"),
        "实际: {summary:?}"
    );
    assert!(summary.contains("query"), "正文应为架构查询输出");
}
