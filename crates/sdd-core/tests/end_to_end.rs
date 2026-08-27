//! 端到端测试：完整工作流 init → archive。

use sdd_core::commands::new::run_new;
use sdd_core::contracts::CommandRequest;
use sdd_core::engines::spec::spec_engine::SpecEngine;
use sdd_core::run;
use serde_json::json;

const FULL_REQUIREMENT: &str = "授权用户通过 POST /orders/{id}/cancel 请求取消待处理订单，入参 order_id，返回 status 和 error_code，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";

fn run_command(
    cwd: &str,
    command: &str,
    args: Option<serde_json::Value>,
) -> sdd_core::contracts::CommandResult {
    run(&CommandRequest {
        command: command.into(),
        cwd: cwd.to_string(),
        args,
    })
    .unwrap_or_else(|e| panic!("命令 {command} 失败: {} ({})", e.message, e.code))
}

fn complete_all_tasks(cwd: &str) {
    for _ in 0..100 {
        let next = run(&CommandRequest {
            command: "build".into(),
            cwd: cwd.to_string(),
            args: Some(json!({ "sub": "next" })),
        });
        let Ok(next) = next else { break };
        let Some(action) = next.action_required else {
            break;
        };
        let is_verify = action.task_id.ends_with("-VERIFY");
        let is_red = action.task_id.ends_with("-RED");
        // 证据/验证命令必须来自 actionRequired 声明的 verification（项目相关）
        let verify_command = action
            .verification
            .first()
            .map(|v| {
                std::iter::once(v.command.as_str())
                    .chain(v.args.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_else(|| "cargo test".to_string());
        let evidence = if is_verify {
            json!([])
        } else {
            json!([{ "type": "command-run", "command": verify_command,
                "output": if is_red { "FAILED: expected" } else { "ok" },
                "passed": !is_red, "expectedFailure": is_red }])
        };
        // 全阶段不重不漏地回传 actionRequired 声明的验证命令；RED 全部按预期失败记录。
        let verification = action
            .verification
            .iter()
            .map(|item| {
                json!({
                    "command": item.command,
                    "args": item.args,
                    "passed": !is_red,
                })
            })
            .collect::<Vec<_>>();
        let result = json!({
            "taskId": action.task_id,
            "status": "completed",
            "evidence": evidence,
            "verification": verification,
            "filesChanged": []
        });
        let result = run(&CommandRequest {
            command: "build".into(),
            cwd: cwd.to_string(),
            args: Some(json!({
                "sub": "complete",
                "task": action.task_id,
                "resultJson": result.to_string(),
            })),
        });
        result.unwrap_or_else(|error| panic!("完成任务失败：{} {}", error.code, error.message));
    }
    assert_eq!(
        sdd_core::state::StateStore::new(cwd.to_string())
            .read()
            .unwrap()
            .current_phase,
        "BUILD_READY"
    );
}

#[test]
fn full_workflow_init_to_archive() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    assert_full_workflow(&dir);
}

#[test]
fn node_fixture_completes_full_workflow() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/node-basic-service");
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::copy(
        fixture.join("package.json"),
        dir.path().join("package.json"),
    )
    .unwrap();
    std::fs::copy(
        fixture.join("src/index.ts"),
        dir.path().join("src/index.ts"),
    )
    .unwrap();
    assert_full_workflow(&dir);
}

fn assert_full_workflow(dir: &tempfile::TempDir) {
    let cwd = dir.path().to_string_lossy().to_string();
    let init = run_command(&cwd, "init", None);
    assert_eq!(init.state, "INDEX_READY");
    assert!(dir.path().join(".sdd/runtime.json").exists());
    // new（完整需求 → SPEC_READY）
    let new = run_new(
        &cwd,
        Some(&json!({ "requirement": FULL_REQUIREMENT })),
        &SpecEngine::new(),
    )
    .unwrap();
    assert!(new.ok);
    assert_eq!(new.state, "SPEC_READY");

    sdd_core::state::RuntimeStore::new(cwd.clone())
        .update(|runtime| {
            let prefix = runtime.index["summary"]
                .as_str()
                .unwrap()
                .lines()
                .next()
                .unwrap();
            runtime.index["summary"] = json!(format!(
                "{prefix}\nsrc/order_service.rs\nsrc/order_service.test.rs\nCargo.toml\n"
            ));
            runtime.index["updatedAt"] = json!("2026-01-01T00:00:00Z");
        })
        .unwrap();

    // design → plan
    let design = run_command(&cwd, "design", None);
    assert!(design.ok);
    assert_eq!(design.state, "DESIGN_READY");
    let plan = run_command(&cwd, "plan", None);
    assert!(plan.ok);
    assert_eq!(plan.state, "PLAN_READY");

    // build 全部任务
    complete_all_tasks(&cwd);

    // verify → review → archive
    let verify = run_command(&cwd, "verify", None);
    assert_eq!(verify.state, "VERIFY_READY");
    let review = run_command(&cwd, "review", None);
    assert_eq!(review.state, "REVIEW_READY");
    let archive = run_command(&cwd, "archive", None);
    assert!(archive.ok);
    assert_eq!(archive.state, "ARCHIVED");

    let _status = run_command(&cwd, "status", None);
    let runtime: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".sdd/runtime.json")).unwrap(),
    )
    .unwrap();
    assert!(runtime["artifacts"]["artifacts"].as_object().unwrap().len() >= 8);
}

#[test]
fn state_machine_rejects_wrong_phase() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    run_command(&cwd, "init", None);
    // 未 new 时 design 应报 E_INVALID_PHASE_COMMAND
    let err = run(&CommandRequest {
        command: "design".into(),
        cwd,
        args: None,
    })
    .unwrap_err();
    assert_eq!(err.code, "E_INVALID_PHASE_COMMAND");
}
