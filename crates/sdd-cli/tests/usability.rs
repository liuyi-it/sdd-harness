//! 使用发布形态的 CLI 和真实 Git/Python 项目验证用户旅程。

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
    let mut args = args.to_vec();
    args.push("--json");
    let output = cli(root, &args);
    assert!(output.status.success(), "{args:?}: {}", combined(&output));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn approve_spec_result(root: &Path, command: &str, result: &str) -> Value {
    approve_spec_result_for(root, command, None, result)
}

fn approve_spec_result_for(
    root: &Path,
    command: &str,
    change_id: Option<&str>,
    result: &str,
) -> Value {
    let mut args = vec![command];
    if let Some(change_id) = change_id {
        args.extend(["--change", change_id]);
    }
    args.extend(["--result-json", result]);
    let action = run(root, &args);
    assert_eq!(action["state"], "SPEC_WAITING_REVIEW");
    assert_eq!(action["actionRequired"]["type"], "AGENT_REVIEW_EXECUTION");
    let required = &action["actionRequired"];
    let review = json!({
        "reviewId": required["reviewId"],
        "targetHash": required["targetHash"],
        "mode": "self",
        "verdict": "READY",
        "summary": "已核对统一规格的需求、验收场景和技术设计。",
        "findings": [],
        "rechecks": []
    });
    let review_json = review.to_string();
    let mut review_args = vec![command];
    if let Some(change_id) = change_id {
        review_args.extend(["--change", change_id]);
    }
    review_args.extend(["--result-json", review_json.as_str()]);
    run(root, &review_args)
}

fn approve_quality_review(root: &Path) -> Value {
    let action = run(root, &["verify"]);
    assert_eq!(action["state"], "QUALITY_WAITING_REVIEW");
    assert_eq!(action["actionRequired"]["type"], "AGENT_REVIEW_EXECUTION");
    assert_eq!(action["actionRequired"]["target"], "QUALITY");
    for field in ["reviewId", "targetHash", "reviewer", "resultSchema"] {
        assert!(!action["actionRequired"][field].is_null());
    }
    let required = &action["actionRequired"];
    let review = json!({
        "reviewId": required["reviewId"],
        "targetHash": required["targetHash"],
        "mode": "self",
        "verdict": "READY",
        "summary": "已核对质量报告和验证证据。",
        "findings": [],
        "rechecks": []
    });
    let review_json = review.to_string();
    run(root, &["verify", "--result-json", review_json.as_str()])
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=SDD Demo",
            "-c",
            "user.email=demo@example.invalid",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        combined(&output)
    );
}

#[test]
fn maven_quality_options_and_goals_are_validated_through_cli() {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-q"]);
    std::fs::write(root.path().join("README.md"), "Maven 验证协议样例\n").unwrap();
    git(root.path(), &["add", "README.md"]);
    git(root.path(), &["commit", "-qm", "初始化 Maven 验证样例"]);
    run(root.path(), &["init"]);
    run(root.path(), &["spec", "用 Maven 验证当前业务变更"]);
    approve_spec_result(root.path(), "spec", SPEC);
    run(root.path(), &["plan"]);

    let mut plan: Value = serde_json::from_str(PLAN).unwrap();
    plan["tasks"][0]["verification"] = json!([{
        "command": "mvn",
        "args": ["-B", "-q", "--settings", "settings.xml", "clean", "verify", "-e", "--color"],
        "expected": "Maven 质量检查通过"
    }]);
    // 仅验证入口协议；真正的 Spring Boot/HTTP 执行由独立宿主试用证明。
    for args in [
        vec!["test", "deploy"],
        vec!["verify", "install"],
        vec!["-B", "test", "help:evaluate"],
        vec!["--settings", "verify"],
        vec!["-s", "-q", "verify"],
        vec!["-q", "clean"],
        vec!["--color", "verify"],
        vec!["-Bq", "verify"],
    ] {
        let mut rejected_plan = plan.clone();
        rejected_plan["tasks"][0]["verification"][0]["args"] = json!(args);
        let output = cli(
            root.path(),
            &[
                "plan",
                "--result-json",
                &rejected_plan.to_string(),
                "--json",
            ],
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            result["error"]["code"], "E_SECURITY_BLOCKED",
            "{args:?}: {result}"
        );
        assert_eq!(result["state"], "PLAN_WAITING_AGENT");
    }
    let accepted = run(root.path(), &["plan", "--result-json", &plan.to_string()]);
    assert_eq!(accepted["state"], "PLAN_READY");
    let task = run(root.path(), &["build", "next"]);
    assert_eq!(
        task["actionRequired"]["verification"][0]["args"],
        plan["tasks"][0]["verification"][0]["args"]
    );
}

#[test]
fn shipping_demo_completes_with_real_evidence_and_resumes_after_revision() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init"]);
    std::fs::write(root.join("README.md"), "# 运费计算 demo\n").unwrap();
    std::fs::write(root.join(".gitignore"), "__pycache__/\n").unwrap();
    git(root, &["add", "README.md", ".gitignore"]);
    git(root, &["commit", "-m", "初始化运费演示"]);

    assert_eq!(run(root, &["init"])["state"], "INDEX_READY");
    run(
        root,
        &[
            "spec",
            "满 100 元免运费，否则 10 元，拒绝负数",
            "--change",
            "shipping",
        ],
    );
    let waiting = run(root, &["status"]);
    assert_eq!(waiting["next"], "sdd spec --change shipping");
    assert_eq!(run(root, &["spec"])["changeId"], "shipping");
    assert_eq!(
        approve_spec_result(root, "spec", SPEC)["state"],
        "SPEC_READY"
    );

    let planning = run(root, &["plan"]);
    assert_eq!(
        planning["actionRequired"]["resultSchema"]["properties"]["tasks"]["items"]["required"][0],
        "id"
    );
    assert!(
        planning["actionRequired"]["resultSchema"]["properties"]["tasks"]["items"]["properties"]
            ["testSeam"]["description"]
            .as_str()
            .unwrap()
            .contains("文件路径")
    );
    assert_eq!(run(root, &["status"])["next"], "sdd plan --change shipping");
    assert_eq!(
        run(root, &["plan"])["actionRequired"],
        planning["actionRequired"]
    );

    // 规划等待时仍能修订；再次输入必须替换等待中的请求，且保留既有规格。
    run(root, &["change", "将门槛改为 200 元"]);
    let revision = run(root, &["change", "最终仍用 100 元，保留负数拒绝规则"]);
    let context = revision["actionRequired"]["contextPack"].as_str().unwrap();
    assert!(context.contains("最终仍用 100 元"));
    assert!(!context.contains("将门槛改为 200 元"));
    assert!(context.contains("修订前规格"));
    assert!(context.contains("REQ-001-SC-002"));
    assert_eq!(
        run(root, &["change"])["actionRequired"],
        revision["actionRequired"]
    );
    let stale_plan = cli(root, &["plan", "--result-json", PLAN, "--json"]);
    assert!(!stale_plan.status.success());
    assert_eq!(
        approve_spec_result(root, "change", SPEC)["state"],
        "SPEC_READY"
    );
    run(root, &["plan"]);

    let python = if cfg!(windows) { "python" } else { "python3" };
    let mut plan: Value = serde_json::from_str(PLAN).unwrap();
    // 计划范围与实施范围必须采用相同 glob 语义。
    plan["tasks"][0]["allowedFiles"] = json!(["*.py"]);
    plan["tasks"][0]["verification"][0]["command"] = json!(python);
    let mut invalid_plan = plan.clone();
    invalid_plan["tasks"][0]
        .as_object_mut()
        .unwrap()
        .remove("verification");
    let rejected = cli(
        root,
        &["plan", "--result-json", &invalid_plan.to_string(), "--json"],
    );
    assert!(!rejected.status.success());
    assert!(combined(&rejected).contains("verification"));
    run(root, &["plan", "--result-json", &plan.to_string()]);
    let build = run(root, &["build", "next"]);
    let required = build["actionRequired"]["resultSchema"]["required"]
        .as_array()
        .unwrap();
    assert!(required.contains(&json!("evidence")));
    assert!(required.contains(&json!("verification")));
    assert_eq!(
        run(root, &["build", "next"])["actionRequired"],
        build["actionRequired"]
    );

    std::fs::write(root.join("test_shipping.py"), TEST).unwrap();
    let red = Command::new(python)
        .args(["-m", "unittest", "-v"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(!red.status.success());
    assert!(
        combined(&red).contains("ModuleNotFoundError"),
        "{}",
        combined(&red)
    );
    std::fs::write(root.join("shipping.py"), IMPLEMENTATION).unwrap();
    let green = Command::new(python)
        .args(["-m", "unittest", "-v"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(green.status.success(), "{}", combined(&green));
    assert!(combined(&green).contains("Ran 2 tests"));
    git(root, &["diff", "--check"]);

    let command = format!("{python} -m unittest -v");
    let result = json!({
        "taskId": "TASK-001", "status": "completed",
        "collaboration": {
            "topology": "inline",
            "lead": "developer",
            "contributions": [],
            "limitations": []
        },
        "filesChanged": ["shipping.py", "test_shipping.py"],
        "evidence": [
            {"type": "command-run", "command": command, "passed": false, "expectedFailure": true, "output": combined(&red)},
            {"type": "command-run", "command": command, "passed": true, "output": combined(&green)}
        ],
        "verification": [{"command": python, "args": ["-m", "unittest", "-v"], "passed": true, "output": combined(&green)}]
    });
    // 相同的拼接文本不代表相同的 argv，不能用另一条命令的结果冒充计划验证。
    let mut wrong_arguments = result.clone();
    wrong_arguments["verification"][0]["args"] = json!(["-m", "unittest -v"]);
    let rejected = cli(
        root,
        &[
            "build",
            "complete",
            "--task",
            "TASK-001",
            "--result-json",
            &wrong_arguments.to_string(),
            "--json",
        ],
    );
    assert!(!rejected.status.success());
    assert!(combined(&rejected).contains("E_SECURITY_BLOCKED"));
    assert_eq!(
        run(
            root,
            &[
                "build",
                "complete",
                "--task",
                "TASK-001",
                "--result-json",
                &result.to_string()
            ]
        )["state"],
        "BUILD_READY"
    );
    assert_eq!(approve_quality_review(root)["state"], "QUALITY_READY");
    // 在真实完成的项目上制造范围问题，验证普通用户能看到阻断原因和处理选择。
    std::fs::write(root.join("unexpected.txt"), "计划外文件\n").unwrap();
    let fixing = combined(&cli(root, &["verify"]));
    assert!(fixing.contains("unexpected.txt"), "{fixing}");
    assert!(!fixing.contains("Context Pack"));
    let fix = run(root, &["verify"]);
    let failed_fix = json!({
        "fixId": fix["actionRequired"]["fixId"], "status": "failed",
        "collaboration": {
            "topology": "inline",
            "lead": "quality",
            "contributions": [],
            "limitations": []
        },
        "filesChanged": [], "verification": result["verification"]
    });
    let blocked = cli(root, &["verify", "--result-json", &failed_fix.to_string()]);
    assert!(!blocked.status.success());
    let feedback = combined(&blocked);
    assert!(feedback.contains("unexpected.txt"), "{feedback}");
    assert!(feedback.contains("授权后继续修复"));
    assert!(feedback.contains("重新验证：sdd verify --change shipping"));
    let blocked_status = combined(&cli(root, &["status"]));
    assert!(
        blocked_status.contains("unexpected.txt"),
        "{blocked_status}"
    );
    assert!(blocked_status.contains("任务进度：1/1 已完成"));
    // 用户选择手动恢复范围后，仍须明确 --continue 授权才能重新进入修复流程。
    std::fs::remove_file(root.join("unexpected.txt")).unwrap();
    let still_blocked = cli(root, &["verify", "--json"]);
    assert!(!still_blocked.status.success());
    let still_blocked_json: Value = serde_json::from_slice(&still_blocked.stdout).unwrap();
    assert_eq!(still_blocked_json["state"], "QUALITY_BLOCKED");
    assert!(still_blocked_json["actionRequired"].is_null());

    let next_fix = run(root, &["verify", "--continue"]);
    assert_eq!(next_fix["state"], "QUALITY_WAITING_FIX");
    assert_eq!(next_fix["actionRequired"]["type"], "AGENT_FIX_EXECUTION");
    assert_eq!(next_fix["actionRequired"]["userAuthorized"], true);
    let retry = Command::new(python)
        .args(["-m", "unittest", "-v"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(retry.status.success(), "{}", combined(&retry));
    assert!(combined(&retry).contains("Ran 2 tests"));
    let retry_result = json!({
        "fixId": next_fix["actionRequired"]["fixId"],
        "status": "completed",
        "collaboration": {
            "topology": "inline",
            "lead": "quality",
            "contributions": [],
            "limitations": []
        },
        "filesChanged": [],
        "verification": [{
            "command": python,
            "args": ["-m", "unittest", "-v"],
            "passed": retry.status.success(),
            "output": combined(&retry)
        }]
    });
    assert_eq!(
        run(
            root,
            &["verify", "--result-json", &retry_result.to_string()]
        )["state"],
        "QUALITY_WAITING_REVIEW"
    );
    assert_eq!(approve_quality_review(root)["state"], "QUALITY_READY");
    let status = combined(&cli(root, &["status"]));
    assert!(status.contains("任务进度：1/1 已完成"), "{status}");
    assert!(!status.contains("JSON 过长"));
    assert_eq!(run(root, &["archive"])["state"], "ARCHIVED");
    let archived_status = combined(&cli(root, &["status", "--change", "shipping"]));
    assert!(
        archived_status.contains("提供可测试的订单运费计算"),
        "{archived_status}"
    );
    let archive = std::fs::read_to_string(root.join(".sdd/changes/shipping/archive.md")).unwrap();
    assert!(archive.contains("已完成任务数：1"));
    assert!(!archive.contains("## [ ] TASK-001"));
    assert!(archive.contains("REQ-001-SC-002"));
    assert!(!root.join(".sdd/changes/shipping/spec.md").exists());
    assert_eq!(
        run(root, &["archive", "--change", "shipping"])["state"],
        "ARCHIVED"
    );
    assert_eq!(run(root, &["status"])["data"]["activeChanges"], json!([]));
}

#[test]
fn terminal_shows_selectable_changes_and_keeps_agent_context_in_json() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    run(root, &["init"]);
    let title = "这是需要展示完整标题的中文需求".repeat(20);
    let first = cli(root, &["spec", &title, "--change", "alpha"]);
    assert!(first.status.success());
    let text = combined(&first);
    assert!(text.contains("等待 Agent 完成规格与技术设计"));
    assert!(text.contains("终端命令不会自行启动 AI"));
    assert!(!text.contains("BEGIN_UNTRUSTED"));
    assert!(!text.contains("Context Pack"));
    run(root, &["spec", "另一个业务需求", "--change", "beta"]);
    let status = combined(&cli(root, &["status"]));
    assert!(status.contains(&title));
    assert!(status.contains("[alpha]"));
    assert!(status.contains("另一个业务需求 [beta]"));
    assert!(status.contains("--change <标识>"));
    assert!(!status.contains("JSON 过长"));
    let ambiguous = cli(root, &["plan", "--json"]);
    assert!(!ambiguous.status.success());
    assert!(combined(&ambiguous).contains("E_CHANGE_SELECTION_REQUIRED"));
    let selected = combined(&cli(root, &["status", "--change", "beta"]));
    assert!(!selected.contains("[alpha]"));
    let raw = run(root, &["spec", "--change", "alpha"]);
    assert!(raw["actionRequired"]["contextPack"]
        .as_str()
        .unwrap()
        .contains(&title));
    assert!(raw["actionRequired"]["resultSchema"].is_object());
}

#[test]
fn human_errors_keep_the_selected_change_and_show_latest_business_titles() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let initialized = combined(&cli(root, &["init"]));
    assert!(initialized.contains("空项目已就绪，可直接描述需求"));
    assert!(!initialized.contains("警告：空项目"));
    run(root, &["spec", "订单导出", "--change", "export"]);
    run(root, &["spec", "客户搜索", "--change", "search"]);
    let ambiguous = cli(root, &["plan"]);
    assert!(!ambiguous.status.success());
    let text = combined(&ambiguous);
    assert!(text.contains("订单导出 [export]"), "{text}");
    assert!(text.contains("客户搜索 [search]"));
    assert!(!text.contains("--json"));

    let wrong_phase = cli(root, &["build", "--change", "export", "--json"]);
    let wrong_phase: Value = serde_json::from_slice(&wrong_phase.stdout).unwrap();
    assert_eq!(wrong_phase["error"]["next"], "sdd spec --change export");
    assert!(!wrong_phase["error"]["message"]
        .as_str()
        .unwrap()
        .contains("SPEC_WAITING_AGENT"));
    let resumed = run(root, &["spec", "--change", "export"]);
    assert_eq!(resumed["changeId"], "export");
    let typo = combined(&cli(root, &["status", "--change", "typo"]));
    assert!(typo.contains("建议：sdd status"), "{typo}");

    assert_eq!(
        approve_spec_result_for(root, "spec", Some("export"), SPEC)["state"],
        "SPEC_READY"
    );
    run(root, &["change", "只导出退款订单", "--change", "export"]);
    let status = run(root, &["status", "--change", "export"]);
    assert_eq!(status["data"]["selectedChange"]["title"], "只导出退款订单");
    let text = combined(&cli(root, &["status"]));
    assert!(text.contains("只导出退款订单 [export]"), "{text}");
    assert!(!text.contains("提供可测试的订单运费计算"));
}

#[test]
fn structure_choice_reaches_the_agent_and_help_explains_first_use() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let help = combined(&cli(root, &["--help"]));
    assert!(help.contains("快速开始"));
    assert!(help.contains("sdd init"));
    assert!(help.contains("CLI 不会自行启动 AI"));
    run(root, &["init", "--structurePolicy", "user-defined"]);
    let action = run(root, &["spec", "实现订单导出", "--change", "export"]);
    let context = action["actionRequired"]["contextPack"].as_str().unwrap();
    assert!(context.contains("目录结构由用户指定"));
    run(root, &["init", "--structurePolicy", "free-design"]);
    let action = run(root, &["spec", "--change", "export"]);
    let context = action["actionRequired"]["contextPack"].as_str().unwrap();
    assert!(context.contains("用户允许 Agent 根据需求设计目录结构"));
    assert!(!context.contains("目录结构由用户指定"));
}

#[test]
fn codebase_text_shows_diagnostics_and_large_results_without_json_truncation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for index in 0..40 {
        std::fs::write(
            root.join(format!("shipping_module_{index:02}.py")),
            "pass\n",
        )
        .unwrap();
    }
    // 独立子进程显式缺少 CodeGraph，稳定检验降级反馈，不修改全局 PATH。
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_sdd"))
            .env("PATH", "")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap()
    };
    let doctor = invoke(&["codebase", "doctor"]);
    assert!(doctor.status.success());
    let text = combined(&doctor);
    assert!(text.contains("不可用"));
    assert!(text.contains("可继续开发"));
    assert!(!text.contains("数据："));
    let query = invoke(&["codebase", "query", "shipping", "--intent", "impact"]);
    assert!(query.status.success());
    let text = combined(&query);
    assert!(text.contains("shipping_module_00.py"), "{text}");
    assert!(text.contains("shipping_module_39.py"));
    assert!(!text.contains("JSON 过长"));
    let raw = invoke(&["codebase", "query", "shipping", "--json"]);
    let raw: Value = serde_json::from_slice(&raw.stdout).unwrap();
    assert_eq!(raw["data"]["degraded"], true);
    assert_eq!(
        raw["data"]["payload"]["files"].as_array().unwrap().len(),
        40
    );
}
