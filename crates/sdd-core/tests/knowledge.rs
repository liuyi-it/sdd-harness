//! 知识图谱 Provider 与路由测试。

use sdd_core::knowledge::codegraph::CodeGraphProvider;
use sdd_core::knowledge::fallback_scan::fallback_scan;
use sdd_core::knowledge::provider::{find_on_path, KnowledgeIntent};
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
    let result = provider.probe(std::time::Duration::from_secs(15));
    assert!(result.available || result.message.is_some());
}

#[test]
fn query_when_unavailable_is_degraded() {
    let provider = CodeGraphProvider::default();
    if !provider.probe(std::time::Duration::from_secs(15)).available {
        let result = provider.query(
            ".",
            KnowledgeIntent::Impact,
            "foo",
            std::time::Duration::from_secs(60),
        );
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
fn initialize_builds_diagnostics_and_summary_without_writing_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let router = KnowledgeRouter::new();
    let index = router.initialize(&cwd, 600_000);
    assert_eq!(index.diagnostics.len(), 1); // CodeGraph 一条诊断
    assert!(!dir.path().join(".sdd/runtime.json").exists());
    let summary = index.summary;
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
    let result = router.query(&cwd, KnowledgeIntent::Impact, "main", 60_000);
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
    let result = fallback_scan(&cwd, KnowledgeIntent::Architecture, "", "测试降级");
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
    let result = fallback_scan(&cwd, KnowledgeIntent::Architecture, "", "测试降级");
    let summary = result
        .payload
        .get("codebaseSummary")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(summary.contains("normal.txt"));
    assert!(!summary.contains("id_rsa"));
    assert!(!summary.contains("secret.pem"));
}

#[test]
fn fallback_scan_filters_file_names_with_the_query() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/order_service.rs"), "").unwrap();
    std::fs::write(dir.path().join("src/payment_service.rs"), "").unwrap();
    let cwd = dir.path().to_string_lossy();

    let result = fallback_scan(&cwd, KnowledgeIntent::Impact, "order service", "测试降级");
    let files = result.payload["files"].as_array().unwrap();
    assert_eq!(files, &[serde_json::json!("src/order_service.rs")]);
}

#[test]
fn fallback_scan_keeps_repository_context_when_no_file_name_matches() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/order_service.rs"), "").unwrap();
    let cwd = dir.path().to_string_lossy();

    let result = fallback_scan(
        &cwd,
        KnowledgeIntent::Impact,
        "unmatched-domain-term",
        "测试降级",
    );
    assert_eq!(
        result.payload["files"].as_array().unwrap(),
        &[serde_json::json!("src/order_service.rs")]
    );
}

#[test]
fn fallback_scan_reports_filesystem_failures() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    let result = fallback_scan(
        &missing.to_string_lossy(),
        KnowledgeIntent::Architecture,
        "",
        "CodeGraph 查询失败",
    );

    assert_eq!(result.payload["scan"]["complete"], false);
    assert_eq!(result.payload["scan"]["issueCount"], 1);
    assert!(result
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("1 个读取错误")));
}

#[cfg(target_os = "linux")]
#[test]
fn fallback_scan_reports_non_utf8_paths_instead_of_lossy_names() {
    use std::os::unix::ffi::OsStringExt;

    let dir = tempfile::tempdir().unwrap();
    let name = std::ffi::OsString::from_vec(vec![b'f', 0xff]);
    std::fs::write(dir.path().join(name), "content").unwrap();

    let result = fallback_scan(
        &dir.path().to_string_lossy(),
        KnowledgeIntent::Architecture,
        "",
        "测试降级",
    );

    assert_eq!(result.payload["scan"]["complete"], false);
    assert_eq!(result.payload["scan"]["issueCount"], 1);
}

#[cfg(unix)]
fn fake_cli() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".codegraph")).unwrap();
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

    std::fs::create_dir(dir.join(".codegraph")).unwrap();
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
    let codegraph = CodeGraphProvider { bin: Some(bin) };
    let root = fake.path().to_string_lossy();

    let codegraph_result = codegraph.query(
        &root,
        KnowledgeIntent::Context,
        "OrderService",
        std::time::Duration::from_secs(60),
    );
    assert_eq!(
        codegraph_result.payload["output"],
        format!("explore OrderService --path {root}")
    );
    let impact_result = codegraph.query(
        &root,
        KnowledgeIntent::Impact,
        "cancel_order",
        std::time::Duration::from_secs(60),
    );
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

    let result = router.query(&root, KnowledgeIntent::Impact, "cancel_order", 60_000);
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

    let _ = router.query(&root, KnowledgeIntent::Impact, "cancel_order", 60_000);
    let _ = router.query(&root, KnowledgeIntent::Impact, "cancel_order", 60_000);

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
fn codegraph_rejects_symlinked_index_directory_without_starting_cli() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), project.path().join(".codegraph")).unwrap();
    let marker = project.path().join("started");
    let bin = project.path().join("codegraph");
    std::fs::write(&bin, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
    let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&bin, permissions).unwrap();
    let provider = CodeGraphProvider { bin: Some(bin) };
    let root = project.path().to_string_lossy();

    let index = provider.index(&root, 1_000);
    assert!(!index.ok);
    assert!(index.reason.unwrap().contains("符号链接"));
    let query = provider.query(
        &root,
        KnowledgeIntent::Impact,
        "order",
        std::time::Duration::from_secs(1),
    );
    assert!(query.degraded);
    assert!(query.reason.unwrap().contains("符号链接"));
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn codegraph_query_requires_index_without_starting_cli() {
    use std::os::unix::fs::PermissionsExt;

    let project = tempfile::tempdir().unwrap();
    let marker = project.path().join("started");
    let bin = project.path().join("codegraph");
    std::fs::write(&bin, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
    let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&bin, permissions).unwrap();
    let provider = CodeGraphProvider { bin: Some(bin) };

    let query = provider.query(
        &project.path().to_string_lossy(),
        KnowledgeIntent::Impact,
        "order",
        std::time::Duration::from_secs(1),
    );

    assert!(query.degraded);
    assert!(query.reason.unwrap().contains("尚未建立索引"));
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn codegraph_index_rejects_success_without_index_directory() {
    use std::os::unix::fs::PermissionsExt;

    let project = tempfile::tempdir().unwrap();
    let bin = project.path().join("codegraph");
    std::fs::write(&bin, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&bin, permissions).unwrap();
    let provider = CodeGraphProvider { bin: Some(bin) };

    let result = provider.index(&project.path().to_string_lossy(), 1_000);

    assert!(!result.ok);
    assert!(result.reason.unwrap().contains("未生成 .codegraph"));
    let result = provider.rebuild(&project.path().to_string_lossy(), 1_000);
    assert!(!result.ok);
    assert!(result.reason.unwrap().contains("未生成 .codegraph"));
}

#[cfg(unix)]
#[test]
fn codegraph_rejects_empty_and_non_utf8_success_output() {
    use std::os::unix::fs::PermissionsExt;

    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join(".codegraph")).unwrap();
    let empty = project.path().join("empty-codegraph");
    std::fs::write(&empty, "#!/bin/sh\nexit 0\n").unwrap();
    let invalid = project.path().join("invalid-codegraph");
    std::fs::write(&invalid, "#!/bin/sh\nprintf '\\377'\n").unwrap();
    for bin in [&empty, &invalid] {
        let mut permissions = std::fs::metadata(bin).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(bin, permissions).unwrap();
    }
    let root = project.path().to_string_lossy();

    let empty_provider = CodeGraphProvider { bin: Some(empty) };
    assert!(
        !empty_provider
            .probe(std::time::Duration::from_secs(1))
            .available
    );
    assert!(
        empty_provider
            .query(
                &root,
                KnowledgeIntent::Impact,
                "order",
                std::time::Duration::from_secs(1),
            )
            .degraded
    );

    let invalid_provider = CodeGraphProvider { bin: Some(invalid) };
    assert!(
        !invalid_provider
            .probe(std::time::Duration::from_secs(1))
            .available
    );
    assert!(
        invalid_provider
            .query(
                &root,
                KnowledgeIntent::Impact,
                "order",
                std::time::Duration::from_secs(1),
            )
            .degraded
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

    let index = router.initialize(&root, 600_000);
    let summary = index.summary;
    // CodeGraph 可用且索引成功：summary 首行标记 codegraph 来源，正文为查询输出
    assert!(
        summary.starts_with("<!-- summary-provider: codegraph degraded=false -->"),
        "实际: {summary:?}"
    );
    assert!(summary.contains("query"), "正文应为架构查询输出");
}

#[cfg(unix)]
#[test]
fn initialize_degrades_diagnostics_when_summary_query_is_empty() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("codegraph");
    std::fs::write(
        &bin,
        "#!/bin/sh\ncase \"$1\" in\n  --version) printf 'codegraph 1.0' ;;\n  init|sync|index) mkdir -p .codegraph ;;\n  query) exit 0 ;;\nesac\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&bin, permissions).unwrap();
    let router = KnowledgeRouter {
        codegraph: CodeGraphProvider { bin: Some(bin) },
    };

    let index = router.initialize(&dir.path().to_string_lossy(), 1_000);

    assert!(index.diagnostics[0].installed);
    assert!(!index.diagnostics[0].indexed);
    assert!(index.diagnostics[0].degraded);
    assert!(index.diagnostics[0]
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("空输出")));
    assert!(index
        .summary
        .starts_with("<!-- summary-provider: fallback-file-scan degraded=true -->"));
}
