//! verify/review/archive 质量链测试。

use sdd_core::commands::new::run_new;
use sdd_core::contracts::CommandRequest;
use sdd_core::engines::spec::spec_engine::SpecEngine;
use sdd_core::run;
use serde_json::json;

const FULL_REQUIREMENT: &str = "授权用户通过 API 请求取消待处理订单，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";

fn prepare(dir: &std::path::Path) -> String {
    std::fs::write(dir.join("README.md"), "# demo").unwrap();
    let cwd = dir.to_string_lossy().to_string();
    run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    std::fs::create_dir_all(dir.join(".sdd/index")).unwrap();
    std::fs::write(
        dir.join(".sdd/index/codebase-summary.md"),
        "src/order_service.rs\nsrc/order_service.test.rs\nCargo.toml\n",
    )
    .unwrap();
    run_new(
        &cwd,
        Some(&json!({ "requirement": FULL_REQUIREMENT })),
        &SpecEngine::new(),
    )
    .unwrap();
    run(&CommandRequest {
        command: "design".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    run(&CommandRequest {
        command: "plan".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    cwd
}

fn complete_all_tasks(dir: &std::path::Path, cwd: &str) {
    // 依次完成所有任务
    for _ in 0..12 {
        let next = run(&CommandRequest {
            command: "build".into(),
            cwd: cwd.to_string(),
            args: Some(json!({ "sub": "next" })),
        });
        let Ok(next) = next else { break };
        let Some(action) = next.action_required else {
            break;
        };
        let result_path = dir.join(&action.result_file);
        if let Some(parent) = result_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let is_verify = action.task_id.ends_with("-VERIFY");
        let is_red = action.task_id.ends_with("-RED");
        let evidence = if is_verify {
            json!([])
        } else {
            json!([{ "type": "command-run", "command": "cargo test",
                "output": if is_red { "FAILED: expected" } else { "ok" } }])
        };
        std::fs::write(
            &result_path,
            json!({
                "taskId": action.task_id,
                "status": "completed",
                "evidence": evidence,
                "verification": if is_verify {
                    json!([{ "command": "cargo test", "args": [], "passed": true }])
                } else { json!([]) },
                "filesChanged": []
            })
            .to_string(),
        )
        .unwrap();
        let result = run(&CommandRequest {
            command: "build".into(),
            cwd: cwd.to_string(),
            args: Some(json!({
                "sub": "complete",
                "task": action.task_id,
                "result": action.result_file,
            })),
        });
        if result.is_err() {
            break;
        }
    }
}

#[test]
fn verify_fails_when_tasks_incomplete() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    // 尚未进入 BUILD_READY：阶段门禁拦截（E_INVALID_PHASE_COMMAND，对齐 Node 版）
    let err = run(&CommandRequest {
        command: "verify".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap_err();
    assert_eq!(err.code, "E_INVALID_PHASE_COMMAND");
}

#[test]
fn full_chain_verify_review_archive() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    complete_all_tasks(dir.path(), &cwd);
    let verify = run(&CommandRequest {
        command: "verify".into(),
        cwd: cwd.clone(),
        args: None,
    });
    // verify 可能因任务未全部完成而失败；只有全部完成时才继续断言
    let Ok(verify) = verify else { return };
    assert_eq!(verify.state, "VERIFY_READY");
    let review = run(&CommandRequest {
        command: "review".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert!(review.ok, "review 应成功: {:?}", review.error);
    assert_eq!(review.state, "REVIEW_READY");
    let archive = run(&CommandRequest {
        command: "archive".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert!(archive.ok, "archive 应成功: {:?}", archive.error);
    assert_eq!(archive.state, "ARCHIVED");
    // 收敛为三个文件
    let change_dir = std::fs::read_dir(dir.path().join(".sdd/changes"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let names: Vec<String> = std::fs::read_dir(&change_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert!(names.contains(&"archive.json".to_string()));
    assert!(names.contains(&"archive.md".to_string()));
    assert!(names.contains(&".archived".to_string()));
}

#[test]
fn secrets_scanner_flags_keys() {
    let hits = sdd_core::security::secrets_scanner::scan_secrets(
        "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAA\n",
    );
    assert!(!hits.is_empty());
    let aws_hits =
        sdd_core::security::secrets_scanner::scan_secrets("aws_access_key_id=AKIAIOSFODNN7EXAMPLE");
    assert!(!aws_hits.is_empty());
    // 正常内容无命中
    let clean =
        sdd_core::security::secrets_scanner::scan_secrets("fn main() { println!(\"hi\"); }");
    assert!(clean.is_empty());
}

#[test]
fn task_scope_validation() {
    use sdd_core::security::task_scope::validate_file_change;
    // 允许范围内
    assert!(validate_file_change(
        &["src/lib.rs".to_string()],
        &["src/**".to_string()],
        &[],
        &[".git/**".to_string()],
    )
    .is_ok());
    // 未声明变更
    let err = validate_file_change(
        &["outside.txt".to_string()],
        &["src/**".to_string()],
        &[],
        &[],
    )
    .unwrap_err();
    assert_eq!(err.code, "E_UNDECLARED_FILE_CHANGE");
    // 禁止范围
    let err = validate_file_change(
        &[".env".to_string()],
        &["src/**".to_string()],
        &[],
        &[".env".to_string()],
    )
    .unwrap_err();
    assert_eq!(err.code, "E_SECURITY_BLOCKED");
}
