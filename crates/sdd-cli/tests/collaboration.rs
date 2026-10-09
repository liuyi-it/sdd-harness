//! AI-DLC 协作协议的真实 CLI 回归。

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{json, Value};

const SPEC: &str = include_str!("../../../fixtures/usability/spec.json");
const PLAN: &str = include_str!("../../../fixtures/usability/plan.json");
const IMPLEMENTATION: &str = include_str!("../../../fixtures/usability/shipping.py");
const TEST: &str = include_str!("../../../fixtures/usability/test_shipping.py");

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sdd"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

fn run(root: &Path, args: &[&str]) -> Value {
    let mut full_args = args.to_vec();
    full_args.push("--json");
    let output = cli(root, &full_args);
    assert!(
        output.status.success(),
        "{full_args:?}: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn failed(root: &Path, args: &[&str]) -> Value {
    let mut full_args = args.to_vec();
    full_args.push("--json");
    let output = cli(root, &full_args);
    assert!(
        !output.status.success(),
        "{full_args:?} 应失败：{}",
        output_text(&output)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn assert_review_action(action: &Value, target: &str) {
    let required = &action["actionRequired"];
    assert_eq!(required["type"], "AGENT_REVIEW_EXECUTION");
    assert_eq!(required["target"], target);
    for field in [
        "reviewId",
        "targetHash",
        "reviewer",
        "independentRequired",
        "resultSchema",
        "resultTransport",
    ] {
        assert!(!required[field].is_null(), "审查行动缺少 {field}");
    }
    assert!(matches!(
        required["reviewer"].as_str(),
        Some("product-reviewer" | "architecture-reviewer")
    ));
    assert_eq!(required["resultTransport"], "inline-json");
    let required_fields = required["resultSchema"]["required"].as_array().unwrap();
    for field in [
        "reviewId",
        "targetHash",
        "mode",
        "verdict",
        "summary",
        "findings",
        "rechecks",
    ] {
        assert!(required_fields.iter().any(|item| item == field));
    }
}

fn review_result(action: &Value, mode: &str, verdict: &str) -> Value {
    let required = &action["actionRequired"];
    let rechecks = action["data"]["findings"]
        .as_array()
        .map(|findings| {
            findings
                .iter()
                .map(|finding| {
                    json!({
                        "findingId": finding["id"],
                        "present": false,
                        "explanation": "已按当前审查目标重新核对，该历史发现已不再存在。"
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut result = json!({
        "reviewId": required["reviewId"],
        "targetHash": required["targetHash"],
        "mode": mode,
        "verdict": verdict,
        "summary": "已按审查目标核对需求、验收场景与技术设计。",
        "findings": [],
        "rechecks": rechecks
    });
    if mode == "self_fallback" {
        result["fallbackReason"] = json!("当前宿主没有可用的独立审查者");
    }
    result
}

fn submit_spec_review(root: &Path, change_id: &str, action: &Value, mode: &str) -> Value {
    let result = review_result(action, mode, "READY");
    let result_json = result.to_string();
    run(
        root,
        &[
            "spec",
            "--change",
            change_id,
            "--result-json",
            result_json.as_str(),
        ],
    )
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn complete_shipping_build(root: &Path) {
    complete_shipping_build_with_plan(root, |_| {});
}

fn complete_shipping_build_with_glob_scope(root: &Path) {
    complete_shipping_build_with_plan(root, |plan| {
        plan["tasks"][0]["allowedFiles"] =
            json!(["shipping.py", "test_shipping.py", "extra.txt", "src/**"]);
    });
}

fn approve_quality_review(root: &Path) -> Value {
    let action = run(root, &["verify"]);
    assert_review_action(&action, "QUALITY");
    let result = review_result(&action, "self", "READY");
    let result_json = result.to_string();
    run(root, &["verify", "--result-json", result_json.as_str()])
}

fn complete_shipping_build_with_plan<F>(root: &Path, adjust_plan: F)
where
    F: FnOnce(&mut Value),
{
    run(root, &["init"]);
    run(
        root,
        &["spec", "提供可测试的订单运费计算", "--change", "shipping"],
    );
    let spec_action = run(
        root,
        &["spec", "--change", "shipping", "--result-json", SPEC],
    );
    assert_eq!(
        submit_spec_review(root, "shipping", &spec_action, "self")["state"],
        "SPEC_READY"
    );

    run(root, &["plan"]);
    let python = if cfg!(windows) { "python" } else { "python3" };
    let mut plan: Value = serde_json::from_str(PLAN).unwrap();
    plan["tasks"][0]["verification"][0]["command"] = json!(python);
    adjust_plan(&mut plan);
    let plan_json = plan.to_string();
    run(root, &["plan", "--result-json", plan_json.as_str()]);
    let build = run(root, &["build", "next"]);
    assert_eq!(build["actionRequired"]["taskId"], "TASK-001");

    std::fs::write(root.join("test_shipping.py"), TEST).unwrap();
    let red = Command::new(python)
        .args(["-m", "unittest", "-v"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(!red.status.success(), "测试先应失败：{}", output_text(&red));
    std::fs::write(root.join("shipping.py"), IMPLEMENTATION).unwrap();
    let green = Command::new(python)
        .args(["-m", "unittest", "-v"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        green.status.success(),
        "测试应通过：{}",
        output_text(&green)
    );

    let command = format!("{python} -m unittest -v");
    let task_result = json!({
        "taskId": "TASK-001",
        "status": "completed",
        "collaboration": {
            "topology": "inline",
            "lead": "developer",
            "contributions": [],
            "limitations": []
        },
        "filesChanged": ["shipping.py", "test_shipping.py"],
        "evidence": [
            {"type": "command-run", "command": command, "passed": false, "expectedFailure": true, "output": output_text(&red)},
            {"type": "command-run", "command": format!("{python} -m unittest -v"), "passed": true, "output": output_text(&green)}
        ],
        "verification": [{"command": python, "args": ["-m", "unittest", "-v"], "passed": true, "output": output_text(&green)}]
    });
    let task_json = task_result.to_string();
    run(
        root,
        &[
            "build",
            "complete",
            "--task",
            "TASK-001",
            "--result-json",
            task_json.as_str(),
        ],
    );
}

#[test]
fn spec_result_enters_review_before_writing_formal_spec() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    run(root, &["init"]);
    run(
        root,
        &["spec", "提供可测试的订单运费计算", "--change", "shipping"],
    );
    let result = run(
        root,
        &["spec", "--change", "shipping", "--result-json", SPEC],
    );

    assert_eq!(result["state"], "SPEC_WAITING_REVIEW");
    assert_review_action(&result, "SPECIFICATION");
    assert_eq!(result["actionRequired"]["type"], "AGENT_REVIEW_EXECUTION");
    assert!(!root.join(".sdd/changes/shipping/spec.md").exists());
}

#[test]
fn review_rejects_stale_binding_and_can_be_redispatched_once_for_incomplete_result() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    run(
        root,
        &["spec", "提供可测试的订单运费计算", "--change", "shipping"],
    );
    let action = run(
        root,
        &["spec", "--change", "shipping", "--result-json", SPEC],
    );
    assert_review_action(&action, "SPECIFICATION");

    let mut stale = review_result(&action, "self", "READY");
    stale["targetHash"] = json!("expired-target-hash");
    let stale_error = failed(
        root,
        &[
            "spec",
            "--change",
            "shipping",
            "--result-json",
            stale.to_string().as_str(),
        ],
    );
    assert!(!stale_error["error"]["code"].is_null());
    assert!(!root.join(".sdd/changes/shipping/spec.md").exists());

    let mut incomplete = review_result(&action, "independent", "NOT_READY");
    incomplete["incomplete"] = json!(true);
    incomplete["findings"] = json!([{
        "severity": "medium",
        "message": "需要补充金额边界的错误处理说明"
    }]);
    let incomplete_json = incomplete.to_string();
    let redispatched = run(
        root,
        &[
            "spec",
            "--change",
            "shipping",
            "--result-json",
            incomplete_json.as_str(),
        ],
    );
    assert_review_action(&redispatched, "SPECIFICATION");
    assert_eq!(
        redispatched["actionRequired"]["reviewId"],
        action["actionRequired"]["reviewId"]
    );
    assert!(!root.join(".sdd/changes/shipping/spec.md").exists());

    let ready = submit_spec_review(root, "shipping", &redispatched, "independent");
    assert_eq!(ready["state"], "SPEC_READY");
    assert!(root.join(".sdd/changes/shipping/spec.md").is_file());
}

#[test]
fn high_risk_spec_requires_two_independent_reviewers_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    run(
        root,
        &["spec", "需要对外部订单契约做变更", "--change", "contract"],
    );

    let mut spec: Value = serde_json::from_str(SPEC).unwrap();
    spec["goal"] = json!("需要对外部订单契约做变更");
    spec["reviewPolicy"]["riskFactors"] = json!(["external-contract"]);
    spec["reviewPolicy"]["rationale"] = json!("会改变外部订单契约，需要独立的产品与架构审查。");
    let spec_json = spec.to_string();
    let first = run(
        root,
        &[
            "spec",
            "--change",
            "contract",
            "--result-json",
            spec_json.as_str(),
        ],
    );
    assert_review_action(&first, "SPECIFICATION");
    assert_eq!(first["actionRequired"]["independentRequired"], true);

    let mut self_review = review_result(&first, "self", "READY");
    self_review["summary"] = json!("自审不能替代高风险变更的独立审查。");
    let rejected = failed(
        root,
        &[
            "spec",
            "--change",
            "contract",
            "--result-json",
            self_review.to_string().as_str(),
        ],
    );
    assert!(!rejected["error"]["code"].is_null());

    let second = submit_spec_review(root, "contract", &first, "independent");
    assert_review_action(&second, "SPECIFICATION");
    assert_eq!(second["actionRequired"]["independentRequired"], true);
    assert_ne!(
        first["actionRequired"]["reviewer"],
        second["actionRequired"]["reviewer"]
    );
    let ready = submit_spec_review(root, "contract", &second, "independent");
    assert_eq!(ready["state"], "SPEC_READY");
    assert!(root.join(".sdd/changes/contract/spec.md").is_file());
}

#[test]
fn quality_not_ready_spends_one_fix_and_returns_to_review() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    complete_shipping_build(root);

    let first = run(root, &["verify"]);
    assert_review_action(&first, "QUALITY");
    let mut not_ready = review_result(&first, "self", "NOT_READY");
    not_ready["findings"] = json!([{
        "severity": "low",
        "message": "语义审查要求补充一个产品说明"
    }]);
    let not_ready_json = not_ready.to_string();
    let fix = run(root, &["verify", "--result-json", not_ready_json.as_str()]);
    assert_eq!(fix["state"], "QUALITY_WAITING_FIX");
    assert_eq!(fix["actionRequired"]["type"], "AGENT_FIX_EXECUTION");

    let python = if cfg!(windows) { "python" } else { "python3" };
    let fix_result = json!({
        "fixId": fix["actionRequired"]["fixId"],
        "status": "completed",
        "collaboration": {
            "topology": "inline",
            "lead": "quality",
            "contributions": [],
            "limitations": []
        },
        "filesChanged": [],
        "verification": [{"command": python, "args": ["-m", "unittest", "-v"], "passed": true, "output": "Ran 2 tests"}]
    });
    let fix_json = fix_result.to_string();
    let second = run(root, &["verify", "--result-json", fix_json.as_str()]);
    assert_review_action(&second, "QUALITY");
    assert_ne!(
        second["actionRequired"]["reviewId"],
        first["actionRequired"]["reviewId"]
    );
    assert_eq!(second["data"]["findings"].as_array().unwrap().len(), 1);

    let ready = review_result(&second, "self", "READY");
    let ready_json = ready.to_string();
    let result = run(root, &["verify", "--result-json", ready_json.as_str()]);
    assert_eq!(result["state"], "QUALITY_READY");
}

#[test]
fn second_incomplete_review_is_blocked_but_same_action_can_be_recovered() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    run(
        root,
        &["spec", "提供可测试的订单运费计算", "--change", "shipping"],
    );
    let first = run(
        root,
        &["spec", "--change", "shipping", "--result-json", SPEC],
    );

    let mut incomplete = review_result(&first, "independent", "NOT_READY");
    incomplete["incomplete"] = json!(true);
    incomplete["findings"] = json!([{
        "severity": "medium",
        "message": "审查尚未完整返回"
    }]);
    let incomplete_json = incomplete.to_string();
    let redispatched = run(
        root,
        &[
            "spec",
            "--change",
            "shipping",
            "--result-json",
            incomplete_json.as_str(),
        ],
    );
    assert_eq!(
        redispatched["actionRequired"]["reviewId"],
        first["actionRequired"]["reviewId"]
    );

    let mut second_incomplete = review_result(&redispatched, "independent", "NOT_READY");
    second_incomplete["incomplete"] = json!(true);
    let second_json = second_incomplete.to_string();
    let blocked_output = cli(
        root,
        &[
            "spec",
            "--change",
            "shipping",
            "--result-json",
            second_json.as_str(),
            "--json",
        ],
    );
    assert!(!blocked_output.status.success());
    let blocked: Value = serde_json::from_slice(&blocked_output.stdout).unwrap();
    assert_eq!(blocked["exitCode"], 8);
    assert_eq!(blocked["ok"], false);
    assert_eq!(
        blocked["actionRequired"]["reviewId"],
        first["actionRequired"]["reviewId"]
    );

    let recovered_output = cli(root, &["spec", "--change", "shipping", "--json"]);
    assert!(!recovered_output.status.success());
    let recovered: Value = serde_json::from_slice(&recovered_output.stdout).unwrap();
    assert_eq!(recovered["exitCode"], 8);
    assert_eq!(
        recovered["actionRequired"]["reviewId"],
        first["actionRequired"]["reviewId"]
    );

    let ready = review_result(&recovered, "independent", "READY");
    let ready_json = ready.to_string();
    let result = run(
        root,
        &[
            "spec",
            "--change",
            "shipping",
            "--result-json",
            ready_json.as_str(),
        ],
    );
    assert_eq!(result["state"], "SPEC_READY");
}

#[test]
fn high_risk_self_fallback_is_recorded_and_warns_after_completion() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    run(
        root,
        &["spec", "需要对外部订单契约做变更", "--change", "contract"],
    );
    let mut spec: Value = serde_json::from_str(SPEC).unwrap();
    spec["goal"] = json!("需要对外部订单契约做变更");
    spec["reviewPolicy"]["riskFactors"] = json!(["external-contract"]);
    spec["reviewPolicy"]["rationale"] =
        json!("需要独立审查；当前环境无可用独立审查者时允许明确降级。");
    let spec_json = spec.to_string();
    let first = run(
        root,
        &[
            "spec",
            "--change",
            "contract",
            "--result-json",
            spec_json.as_str(),
        ],
    );
    let second = submit_spec_review(root, "contract", &first, "self_fallback");
    assert_review_action(&second, "SPECIFICATION");
    let ready = submit_spec_review(root, "contract", &second, "independent");
    assert_eq!(ready["state"], "SPEC_READY");
    assert_eq!(ready["warnings"][0]["code"], "W_REVIEW_SELF_FALLBACK");
}

#[test]
fn review_risk_escalation_redispatches_independent_reviewer_and_rejects_old_hash() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    run(
        root,
        &["spec", "提供可测试的订单运费计算", "--change", "shipping"],
    );
    let first = run(
        root,
        &["spec", "--change", "shipping", "--result-json", SPEC],
    );
    let mut escalated = review_result(&first, "self", "READY");
    escalated["riskFactors"] = json!(["concurrency"]);
    let escalated_json = escalated.to_string();
    let second = run(
        root,
        &[
            "spec",
            "--change",
            "shipping",
            "--result-json",
            escalated_json.as_str(),
        ],
    );
    assert_review_action(&second, "SPECIFICATION");
    assert_eq!(second["actionRequired"]["independentRequired"], true);
    assert_ne!(
        second["actionRequired"]["reviewId"],
        first["actionRequired"]["reviewId"]
    );
    assert_ne!(
        second["actionRequired"]["targetHash"],
        first["actionRequired"]["targetHash"]
    );

    let stale = failed(
        root,
        &[
            "spec",
            "--change",
            "shipping",
            "--result-json",
            review_result(&first, "self", "READY").to_string().as_str(),
        ],
    );
    assert_eq!(stale["error"]["code"], "E_QUALITY_REQUIRED");

    let third = submit_spec_review(root, "shipping", &second, "independent");
    assert_review_action(&third, "SPECIFICATION");
    assert_eq!(third["actionRequired"]["reviewer"], "architecture-reviewer");
    assert_eq!(third["actionRequired"]["independentRequired"], true);
    let ready = submit_spec_review(root, "shipping", &third, "independent");
    assert_eq!(ready["state"], "SPEC_READY");
}

#[test]
fn revision_invalidates_old_review_and_requires_a_new_candidate_review() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    run(
        root,
        &["spec", "提供可测试的订单运费计算", "--change", "shipping"],
    );
    let old_action = run(
        root,
        &["spec", "--change", "shipping", "--result-json", SPEC],
    );
    let old_review = review_result(&old_action, "self", "READY");
    let old_review_json = old_review.to_string();

    run(
        root,
        &["change", "修订为会员订单的运费计算", "--change", "shipping"],
    );
    let stale = failed(
        root,
        &[
            "change",
            "--change",
            "shipping",
            "--result-json",
            old_review_json.as_str(),
        ],
    );
    assert!(!stale["error"]["code"].is_null());

    let new_spec_json = SPEC.to_string();
    let new_action = run(
        root,
        &[
            "change",
            "--change",
            "shipping",
            "--result-json",
            new_spec_json.as_str(),
        ],
    );
    assert_review_action(&new_action, "SPECIFICATION");
    assert_ne!(
        new_action["actionRequired"]["reviewId"],
        old_action["actionRequired"]["reviewId"]
    );
    assert_eq!(
        submit_spec_review(root, "shipping", &new_action, "self")["state"],
        "SPEC_READY"
    );
}

#[test]
fn archive_rejects_post_review_workspace_change_until_verify_and_review_run_again() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    complete_shipping_build(root);
    let quality = run(root, &["verify"]);
    let quality_ready = review_result(&quality, "self", "READY");
    let quality_ready_json = quality_ready.to_string();
    assert_eq!(
        run(
            root,
            &["verify", "--result-json", quality_ready_json.as_str()]
        )["state"],
        "QUALITY_READY"
    );

    std::fs::OpenOptions::new()
        .append(true)
        .open(root.join("shipping.py"))
        .unwrap()
        .write_all("\n# 审查后修改\n".as_bytes())
        .unwrap();
    let rejected = failed(root, &["archive"]);
    assert_eq!(rejected["error"]["code"], "E_QUALITY_REQUIRED");

    let recheck = run(root, &["verify"]);
    assert_review_action(&recheck, "QUALITY");
    let recheck_result = review_result(&recheck, "self", "READY");
    let recheck_json = recheck_result.to_string();
    assert_eq!(
        run(root, &["verify", "--result-json", recheck_json.as_str()])["state"],
        "QUALITY_READY"
    );
    assert_eq!(run(root, &["archive"])["state"], "ARCHIVED");
}

#[test]
fn no_git_scope_hash_binds_exact_and_glob_additions_and_deletions_before_archive() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    complete_shipping_build_with_glob_scope(root);

    let initial = approve_quality_review(root);
    assert_eq!(initial["state"], "QUALITY_READY");
    assert!(initial["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["code"] == "W_NO_GIT_SCOPE_CHECK"));

    std::fs::write(root.join("extra.txt"), "exact path addition\n").unwrap();
    let exact_add = failed(root, &["archive"]);
    assert_eq!(exact_add["error"]["code"], "E_QUALITY_REQUIRED");
    assert_eq!(approve_quality_review(root)["state"], "QUALITY_READY");

    std::fs::remove_file(root.join("extra.txt")).unwrap();
    let exact_delete = failed(root, &["archive"]);
    assert_eq!(exact_delete["error"]["code"], "E_QUALITY_REQUIRED");
    assert_eq!(approve_quality_review(root)["state"], "QUALITY_READY");

    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join("src/extra.py"), "VALUE = 1\n").unwrap();
    let glob_add = failed(root, &["archive"]);
    assert_eq!(glob_add["error"]["code"], "E_QUALITY_REQUIRED");
    assert_eq!(approve_quality_review(root)["state"], "QUALITY_READY");

    std::fs::remove_file(root.join("src/extra.py")).unwrap();
    let glob_delete = failed(root, &["archive"]);
    assert_eq!(glob_delete["error"]["code"], "E_QUALITY_REQUIRED");

    // 无 Git 时 Core 只能绑定计划范围，计划外文件由质量报告的能力边界明确标记。
    std::fs::write(root.join("unplanned.txt"), "计划外文件\n").unwrap();
    let unplanned = approve_quality_review(root);
    assert_eq!(unplanned["state"], "QUALITY_READY");
    assert!(unplanned["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["code"] == "W_NO_GIT_SCOPE_CHECK"));
    std::fs::remove_file(root.join("unplanned.txt")).unwrap();
}

#[cfg(unix)]
#[test]
fn no_git_rejects_directory_symlink_prefixes_without_entering_review() {
    use std::os::unix::fs::symlink;

    fn complete_with_alias_scope(root: &Path) {
        complete_shipping_build_with_plan(root, |plan| {
            plan["tasks"][0]["allowedFiles"] =
                json!(["shipping.py", "test_shipping.py", "alias/**"]);
        });
    }

    fn assert_rejected(root: &Path) {
        let rejected = failed(root, &["verify"]);
        assert_eq!(rejected["error"]["code"], "E_PATH_OUTSIDE_REPO");
        assert!(rejected["actionRequired"].is_null());
    }

    let external_dir = tempfile::tempdir().unwrap();
    let external_project = tempfile::tempdir().unwrap();
    complete_with_alias_scope(external_project.path());
    symlink(external_dir.path(), external_project.path().join("alias")).unwrap();
    assert_rejected(external_project.path());

    let internal_project = tempfile::tempdir().unwrap();
    let internal_target = internal_project.path().join("internal-target");
    std::fs::create_dir(&internal_target).unwrap();
    complete_with_alias_scope(internal_project.path());
    symlink(&internal_target, internal_project.path().join("alias")).unwrap();
    assert_rejected(internal_project.path());
}

#[test]
fn not_ready_spec_finding_requires_recheck_after_candidate_regeneration() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    run(
        root,
        &["spec", "提供可测试的订单运费计算", "--change", "shipping"],
    );
    let first = run(
        root,
        &["spec", "--change", "shipping", "--result-json", SPEC],
    );
    let mut not_ready = review_result(&first, "self", "NOT_READY");
    not_ready["findings"] = json!([{
        "severity": "medium",
        "message": "规格需要补充会员订单的边界说明"
    }]);
    let not_ready_json = not_ready.to_string();
    let waiting_agent = run(
        root,
        &[
            "spec",
            "--change",
            "shipping",
            "--result-json",
            not_ready_json.as_str(),
        ],
    );
    assert_eq!(waiting_agent["state"], "SPEC_WAITING_AGENT");
    assert_eq!(
        waiting_agent["actionRequired"]["type"],
        "AGENT_PHASE_EXECUTION"
    );
    assert!(!root.join(".sdd/changes/shipping/spec.md").exists());

    let regenerated = run(
        root,
        &["spec", "--change", "shipping", "--result-json", SPEC],
    );
    assert_review_action(&regenerated, "SPECIFICATION");
    assert_ne!(
        regenerated["actionRequired"]["reviewId"],
        first["actionRequired"]["reviewId"]
    );
    assert_eq!(regenerated["data"]["findings"].as_array().unwrap().len(), 1);
    let ready = review_result(&regenerated, "self", "READY");
    let ready_json = ready.to_string();
    assert_eq!(
        run(
            root,
            &[
                "spec",
                "--change",
                "shipping",
                "--result-json",
                ready_json.as_str()
            ]
        )["state"],
        "SPEC_READY"
    );
}

#[test]
fn cli_rejects_invalid_collaboration_topologies_at_phase_boundaries() {
    let spec_dir = tempfile::tempdir().unwrap();
    let spec_root = spec_dir.path();
    run(spec_root, &["init"]);
    run(
        spec_root,
        &["spec", "提供可测试的订单运费计算", "--change", "shipping"],
    );
    let mut invalid_spec: Value = serde_json::from_str(SPEC).unwrap();
    invalid_spec["collaboration"] = json!({
        "topology": "pipeline",
        "lead": "product",
        "contributions": [
            {
                "id": "product-input",
                "role": "product",
                "summary": "整理需求",
                "inputs": [],
                "feedback": []
            },
            {
                "id": "architecture-input",
                "role": "architect",
                "summary": "检查边界",
                "inputs": [],
                "feedback": []
            }
        ],
        "limitations": []
    });
    let invalid_spec_json = invalid_spec.to_string();
    let spec_error = failed(
        spec_root,
        &[
            "spec",
            "--change",
            "shipping",
            "--result-json",
            invalid_spec_json.as_str(),
        ],
    );
    assert_eq!(spec_error["error"]["code"], "E_INVALID_PHASE_COMMAND");

    let plan_dir = tempfile::tempdir().unwrap();
    let plan_root = plan_dir.path();
    run(plan_root, &["init"]);
    run(
        plan_root,
        &["spec", "提供可测试的订单运费计算", "--change", "shipping"],
    );
    let spec_action = run(
        plan_root,
        &["spec", "--change", "shipping", "--result-json", SPEC],
    );
    assert_eq!(
        submit_spec_review(plan_root, "shipping", &spec_action, "self")["state"],
        "SPEC_READY"
    );
    run(plan_root, &["plan"]);
    let mut invalid_plan: Value = serde_json::from_str(PLAN).unwrap();
    invalid_plan["collaboration"] = json!({
        "topology": "mob",
        "lead": "architect",
        "contributions": [
            {
                "id": "developer-a",
                "role": "developer",
                "summary": "实现方案",
                "inputs": [],
                "feedback": []
            },
            {
                "id": "developer-b",
                "role": "developer",
                "summary": "复核方案",
                "inputs": [],
                "feedback": []
            }
        ],
        "limitations": []
    });
    let invalid_plan_json = invalid_plan.to_string();
    let plan_error = failed(
        plan_root,
        &["plan", "--result-json", invalid_plan_json.as_str()],
    );
    assert_eq!(plan_error["error"]["code"], "E_INVALID_PHASE_COMMAND");

    let task_dir = tempfile::tempdir().unwrap();
    let task_root = task_dir.path();
    run(task_root, &["init"]);
    run(
        task_root,
        &["spec", "提供可测试的订单运费计算", "--change", "shipping"],
    );
    let task_spec = run(
        task_root,
        &["spec", "--change", "shipping", "--result-json", SPEC],
    );
    submit_spec_review(task_root, "shipping", &task_spec, "self");
    run(task_root, &["plan"]);
    let plan_json = PLAN.to_string();
    run(task_root, &["plan", "--result-json", plan_json.as_str()]);
    let task_action = run(task_root, &["build", "next"]);
    let invalid_task = json!({
        "taskId": task_action["actionRequired"]["taskId"],
        "status": "completed",
        "collaboration": {
            "topology": "subagent",
            "lead": "developer",
            "contributions": [],
            "limitations": []
        },
        "evidence": [{
            "type": "command-run",
            "command": "python3 -m unittest -v",
            "passed": true,
            "output": "通过"
        }],
        "verification": [{
            "command": "python3",
            "args": ["-m", "unittest", "-v"],
            "passed": true,
            "output": "通过"
        }],
        "filesChanged": []
    });
    let invalid_task_json = invalid_task.to_string();
    let task_error = failed(
        task_root,
        &[
            "build",
            "complete",
            "--task",
            task_action["actionRequired"]["taskId"].as_str().unwrap(),
            "--result-json",
            invalid_task_json.as_str(),
        ],
    );
    assert_eq!(task_error["error"]["code"], "E_TDD_EVIDENCE_REQUIRED");

    let fix_dir = tempfile::tempdir().unwrap();
    let fix_root = fix_dir.path();
    complete_shipping_build(fix_root);
    let quality_action = run(fix_root, &["verify"]);
    let mut not_ready = review_result(&quality_action, "self", "NOT_READY");
    not_ready["findings"] = json!([{
        "severity": "medium",
        "message": "协作结果需要补充独立复核"
    }]);
    let not_ready_json = not_ready.to_string();
    let fix_action = run(
        fix_root,
        &["verify", "--result-json", not_ready_json.as_str()],
    );
    let invalid_fix = json!({
        "fixId": fix_action["actionRequired"]["fixId"],
        "status": "failed",
        "collaboration": {
            "topology": "subagent",
            "lead": "quality",
            "contributions": [],
            "limitations": []
        },
        "filesChanged": [],
        "verification": [{
            "command": "python3",
            "args": ["-m", "unittest", "-v"],
            "passed": false,
            "output": "仍有失败"
        }]
    });
    let invalid_fix_json = invalid_fix.to_string();
    let fix_error = failed(
        fix_root,
        &["verify", "--result-json", invalid_fix_json.as_str()],
    );
    assert_eq!(fix_error["error"]["code"], "E_QUALITY_FAILED");
}

#[test]
fn failed_quality_fix_keeps_failed_verification_and_requires_continue_authorization() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    complete_shipping_build(root);

    let first = run(root, &["verify"]);
    let mut not_ready = review_result(&first, "self", "NOT_READY");
    not_ready["findings"] = json!([{
        "severity": "medium",
        "message": "语义审查发现订单边界说明仍不完整"
    }]);
    let not_ready_json = not_ready.to_string();
    let fix = run(root, &["verify", "--result-json", not_ready_json.as_str()]);
    assert_eq!(fix["state"], "QUALITY_WAITING_FIX");

    let failed_fix = json!({
        "fixId": fix["actionRequired"]["fixId"],
        "status": "failed",
        "collaboration": {
            "topology": "inline",
            "lead": "quality",
            "contributions": [],
            "limitations": ["修复后真实验证仍有失败"]
        },
        "filesChanged": [],
        "verification": [{
            "command": fix["actionRequired"]["verification"][0]["command"],
            "args": fix["actionRequired"]["verification"][0]["args"],
            "passed": false,
            "output": "Ran 2 tests; 1 failed"
        }]
    });
    // 命令必须来自派发包；Windows 的计划使用 python，其他平台使用 python3。
    let mut mismatched_fix = failed_fix.clone();
    mismatched_fix["verification"][0]["command"] =
        if failed_fix["verification"][0]["command"] == "python" {
            json!("python3")
        } else {
            json!("python")
        };
    let rejected = failed(
        root,
        &["verify", "--result-json", &mismatched_fix.to_string()],
    );
    assert_eq!(rejected["error"]["code"], "E_QUALITY_FAILED");
    assert_eq!(rejected["state"], "QUALITY_WAITING_FIX");

    let failed_fix_json = failed_fix.to_string();
    let blocked = failed(root, &["verify", "--result-json", failed_fix_json.as_str()]);
    assert_eq!(blocked["state"], "QUALITY_BLOCKED");
    assert_eq!(blocked["data"]["report"]["passed"], false);
    assert!(blocked["data"]["report"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["message"] == "语义审查发现订单边界说明仍不完整"));
    assert!(blocked["next"].as_str().unwrap().contains("--continue"));

    let still_blocked = failed(root, &["verify"]);
    assert_eq!(still_blocked["state"], "QUALITY_BLOCKED");
    assert!(still_blocked["actionRequired"].is_null());

    let next_fix = run(root, &["verify", "--continue"]);
    assert_eq!(next_fix["state"], "QUALITY_WAITING_FIX");
    assert_eq!(next_fix["actionRequired"]["type"], "AGENT_FIX_EXECUTION");
    assert_eq!(next_fix["actionRequired"]["userAuthorized"], true);
}
