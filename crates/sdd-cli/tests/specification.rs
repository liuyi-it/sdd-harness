//! 验证规格规则的真实 CLI 下发和修订持久化，不模拟模型理解或用户确认。

use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};

const POLICY: &str = include_str!("../../../assets/policies/specification.md");
const WORKSPACE: &str = include_str!("../../../assets/policies/workspace.md");
const SPEC: &str = include_str!("../../../fixtures/usability/spec.json");

fn run(root: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_sdd"))
        .current_dir(root)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn assert_policy(action: &Value) {
    let context = action["contextPack"].as_str().unwrap();
    // 整份受控资产实际下发，并且位于不可信代码材料边界之外。
    assert!(context.ends_with(POLICY));
    let boundary = context.find("END_UNTRUSTED_CODEBASE_CONTEXT").unwrap();
    assert!(context.find(POLICY).unwrap() > boundary);
    assert!(context.find(WORKSPACE).unwrap() > boundary);
    assert_eq!(context.matches(WORKSPACE).count(), 1);
}

fn approve_spec(root: &Path, command: &str, result: &str) -> Value {
    let action = run(root, &[command, "--result-json", result]);
    assert_eq!(action["state"], "SPEC_WAITING_REVIEW");
    assert_eq!(action["actionRequired"]["type"], "AGENT_REVIEW_EXECUTION");
    approve_review(root, command, &action)
}

fn approve_review(root: &Path, command: &str, action: &Value) -> Value {
    let required = &action["actionRequired"];
    let review = json!({
        "reviewId": required["reviewId"],
        "targetHash": required["targetHash"],
        "mode": "self",
        "verdict": "READY",
        "summary": "已核对统一规格的需求、场景和技术设计。",
        "findings": [],
        "rechecks": []
    });
    let review_json = review.to_string();
    run(root, &[command, "--result-json", &review_json])
}

#[test]
fn both_hosts_receive_policy_on_start_resume_and_revision() {
    for host in ["codex", "omp"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        run(root, &["init", "--host-adapter", host]);
        let first = run(
            root,
            &["spec", "满 100 元免运费，拒绝负数", "--change", "shipping"],
        );
        assert_policy(&first["actionRequired"]);
        assert!(!root.join(".sdd/changes/shipping/spec.md").exists());
        assert_eq!(
            run(root, &["spec"])["actionRequired"],
            first["actionRequired"]
        );

        assert_eq!(approve_spec(root, "spec", SPEC)["state"], "SPEC_READY");
        run(root, &["plan"]);
        let revised = run(root, &["change", "门槛改为 200 元，负数仍拒绝"]);
        let action = &revised["actionRequired"];
        assert_policy(action);
        let context = action["contextPack"].as_str().unwrap();
        assert!(context.contains("门槛改为 200 元，负数仍拒绝"));
        // 对完整原规格做比较，避免仅保留摘要而漏掉依据、场景或设计。
        let previous: Value = serde_json::from_str(SPEC).unwrap();
        // 持久化记录包含派生字段；从上下文中读取该记录并核对全部输入字段。
        let start = context.find("\n{\n").unwrap() + 1;
        let mut stream = serde_json::Deserializer::from_str(&context[start..]).into_iter::<Value>();
        let record = stream.next().unwrap().unwrap();
        for (key, value) in previous.as_object().unwrap() {
            assert_eq!(&record[key], value, "缺少旧规格字段 {key}");
        }
        assert_eq!(run(root, &["change"])["actionRequired"], *action);
    }
}

#[test]
fn explicit_target_keeps_spec_out_of_the_session_directory() {
    let session = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let existing = session.path().join("notes.md");
    std::fs::write(&existing, "用户已有材料").unwrap();
    let session_entries = || {
        std::fs::read_dir(session.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>()
    };
    let before = session_entries();

    // 宿主显式传入目标 cwd；结果在内存中回传，不创建工作草稿。
    run(target.path(), &["init"]);
    let action = run(
        target.path(),
        &["spec", "实现运费计算", "--change", "shipping"],
    );
    assert_policy(&action["actionRequired"]);
    assert_eq!(
        approve_spec(target.path(), "spec", SPEC)["state"],
        "SPEC_READY"
    );

    assert!(target
        .path()
        .join(".sdd/changes/shipping/spec.md")
        .is_file());
    assert_eq!(session_entries(), before);
    assert_eq!(std::fs::read_to_string(existing).unwrap(), "用户已有材料");
    for root in [session.path(), target.path()] {
        for directory in ["work", "outputs"] {
            assert!(!root.join(directory).exists());
        }
    }
}

#[test]
fn revised_result_replaces_requirements_scenarios_and_design_together() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    run(root, &["spec", "实现运费计算", "--change", "shipping"]);
    assert_eq!(approve_spec(root, "spec", SPEC)["state"], "SPEC_READY");
    run(root, &["change", "门槛改为 200 元，负数仍拒绝"]);

    // 固定结果只测试传输/渲染，不用它宣称模型能正确理解真实会话。
    let mut revised: Value = serde_json::from_str(SPEC).unwrap();
    revised["scope"]["included"] = json!(["计算满 200 元免运费规则并拒绝负数金额"]);
    revised["model"]["requirements"][0]["statement"] = json!(
        "依据用户修订：非负金额不足 200 元收取 10 元，满 200 元免运费；保留负数抛出 ValueError。"
    );
    revised["model"]["requirements"][0]["scenarios"][0]["given"] =
        json!(["金额分别为 0、199、200、201 元"]);
    revised["model"]["technicalDesign"]["decisions"][0]["decision"] =
        json!("shipping_fee 保持接口，免运费门槛改为 200 元。");
    revised["model"]["technicalDesign"]["decisions"][0]["rationale"] =
        json!("依据本轮用户明确修订；保留其他已确认行为");
    revised["model"]["technicalDesign"]["interfaces"] =
        json!(["shipping_fee 接受整数元金额，满 200 元返回 0，否则返回 10；负数抛出 ValueError。"]);
    let revised_json = revised.to_string();
    let revised_action = run(root, &["change", "--result-json", &revised_json]);
    assert_eq!(revised_action["state"], "SPEC_WAITING_REVIEW");
    assert_eq!(
        approve_review(root, "change", &revised_action)["state"],
        "SPEC_READY"
    );

    let markdown = std::fs::read_to_string(root.join(".sdd/changes/shipping/spec.md")).unwrap();
    for value in [
        "依据用户修订：非负金额不足 200 元",
        "金额分别为 0、199、200、201 元",
        "免运费门槛改为 200 元",
        "依据本轮用户明确修订",
        "满 200 元返回 0",
        "REQ-001-SC-002：拒绝负数",
        "金额为 -1",
    ] {
        assert!(markdown.contains(value), "缺少 {value}");
    }
    assert!(!markdown.contains("100 元"));
    assert!(!markdown.contains("0、99、100、101"));
    let next = run(root, &["change", "保留新规则，补充边界"]);
    let context = next["actionRequired"]["contextPack"].as_str().unwrap();
    assert!(context.contains("依据本轮用户明确修订"));
    assert!(context.contains("满 200 元返回 0"));
    assert!(!context.contains("100 元"));
}
