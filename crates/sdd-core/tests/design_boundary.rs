//! design 命令的不可信上下文边界测试：
//! 代码库摘要必须以 BEGIN/END_UNTRUSTED_CODEBASE_CONTEXT 包裹，
//! END 标记需转义、超长摘要按字符截断 8192（与 build.rs 的 Context Pack 一致）。

use sdd_core::contracts::CommandRequest;
use sdd_core::run;
use serde_json::json;

const FULL_REQUIREMENT: &str = "授权用户通过 POST /orders/{id}/cancel 请求取消待处理订单，入参 order_id，返回 status 和 error_code，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";

const BEGIN_MARKER: &str = "BEGIN_UNTRUSTED_CODEBASE_CONTEXT";
const END_MARKER: &str = "END_UNTRUSTED_CODEBASE_CONTEXT";

fn init_and_new(dir: &std::path::Path) -> String {
    std::fs::write(dir.join("README.md"), "# demo").unwrap();
    let cwd = dir.to_string_lossy().to_string();
    run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    let result = run(&CommandRequest {
        command: "new".into(),
        cwd: cwd.clone(),
        args: Some(json!({ "requirement": FULL_REQUIREMENT })),
    })
    .unwrap();
    assert_eq!(result.state, "SPEC_READY");
    cwd
}

#[test]
fn design_wraps_and_truncates_codebase_summary() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = init_and_new(dir.path());

    // 构造超长摘要：开头放转义哨兵与边界标记，末尾放截断哨兵（位置远超 8192）。
    let mut summary = format!("SENTINEL-START-在开头\n{END_MARKER}\n");
    while summary.chars().count() < 9_000 {
        summary.push_str("代码库上下文填充");
    }
    summary.push_str("SENTINEL-END-在末尾");
    sdd_core::state::RuntimeStore::new(cwd.clone())
        .update(|runtime| {
            let prefix = runtime.index["summary"]
                .as_str()
                .unwrap()
                .lines()
                .next()
                .unwrap();
            runtime.index["summary"] = json!(format!("{prefix}\n{summary}"));
            runtime.index["updatedAt"] = json!("2026-01-01T00:00:00Z");
        })
        .unwrap();

    let result = run(&CommandRequest {
        command: "design".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert_eq!(result.state, "DESIGN_READY");

    let change_id = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap()
        .current_change_id
        .unwrap();
    // design.md 与机器设计字段都包含边界标记
    let design_md = std::fs::read_to_string(
        dir.path()
            .join(".sdd/changes")
            .join(&change_id)
            .join("design.md"),
    )
    .unwrap();
    let machine_design = sdd_core::state::RuntimeStore::new(cwd)
        .read()
        .unwrap()
        .changes[&change_id]["design"]
        .as_str()
        .unwrap()
        .to_string();
    for document in [&design_md, &machine_design] {
        assert!(document.contains(BEGIN_MARKER), "缺少 BEGIN 边界标记");
        assert!(document.contains(END_MARKER), "缺少 END 边界标记");
        assert!(
            document.contains("ESCAPED_END_UNTRUSTED_CODEBASE_CONTEXT"),
            "摘要内的 END 标记应被转义"
        );
        assert!(document.contains("SENTINEL-START-在开头"));
        assert!(
            !document.contains("SENTINEL-END-在末尾"),
            "超长摘要应被截断，末尾哨兵不应出现"
        );
    }

    // 包裹块内的摘要长度 ≤ 8192 字符：取第一个 BEGIN 到其后紧邻的 END 标记之间
    // （转义后的 END 标记带 ESCAPED_ 前缀，不会误匹配带换行的真实 END）。
    let begin = design_md.find(BEGIN_MARKER).unwrap() + BEGIN_MARKER.len();
    let rest = &design_md[begin..];
    let end = rest
        .find(&format!("\n{END_MARKER}"))
        .expect("包裹块应有真实 END 标记");
    let inner = rest[..end].trim_matches('\n');
    assert!(
        inner.chars().count() <= 8_192,
        "摘要应被截断到 8192 字符内，实际 {}",
        inner.chars().count()
    );
}
