//! verify/review/archive 质量链测试。

use sdd_core::commands::new::run_new;
use sdd_core::contracts::CommandRequest;
use sdd_core::engines::spec::spec_engine::SpecEngine;
use sdd_core::run;
use serde_json::json;
use std::io::Write;
use std::process::Command;

const FULL_REQUIREMENT: &str = "授权用户通过 POST /orders/{id}/cancel 请求取消待处理订单，入参 order_id，返回 status 和 error_code，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";
const REVISED_REQUIREMENT: &str =
    "授权用户通过 PATCH /orders/{id} 请求更新待处理订单，入参 order_id 和 status，返回 status 和 error_code，返回更新成功并写审计日志，需要自动化测试覆盖成功与失败";

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} 执行失败");
}

fn prepare(dir: &std::path::Path) -> String {
    std::fs::write(dir.join("README.md"), "# demo").unwrap();
    let cwd = dir.to_string_lossy().to_string();
    run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    sdd_core::state::runtime_store::write_index(
        &cwd,
        json!([]),
        "src/order_service.rs\nsrc/order_service.test.rs\nCargo.toml\n".to_string(),
    )
    .unwrap();
    run_new(
        &cwd,
        Some(&json!({ "requirement": FULL_REQUIREMENT })),
        &SpecEngine::new(),
    )
    .unwrap();
    let change_id = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap()
        .current_change_id
        .unwrap();
    run(&CommandRequest {
        command: "change".into(),
        cwd: cwd.clone(),
        args: Some(json!({
            "changeId": change_id,
            "requirement": REVISED_REQUIREMENT,
        })),
    })
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

fn complete_all_tasks(_dir: &std::path::Path, cwd: &str) {
    // 依次完成所有任务
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
        let evidence = if is_verify {
            json!([])
        } else {
            json!([{ "type": "command-run", "command": "cargo test",
                "output": if is_red { "FAILED: expected" } else { "ok" },
                "passed": !is_red, "expectedFailure": is_red }])
        };
        let result_json = json!({
            "taskId": action.task_id,
            "status": "completed",
            "evidence": evidence,
            "verification": if is_verify {
                json!([{ "command": "cargo test", "args": [], "passed": true }])
            } else { json!([]) },
            "filesChanged": []
        })
        .to_string();
        let result = run(&CommandRequest {
            command: "build".into(),
            cwd: cwd.to_string(),
            args: Some(json!({
                "sub": "complete",
                "task": action.task_id,
                "resultJson": result_json,
            })),
        });
        result.unwrap_or_else(|error| panic!("完成任务失败：{} {}", error.code, error.message));
    }
    let state = sdd_core::state::StateStore::new(cwd.to_string())
        .read()
        .unwrap();
    assert_eq!(state.current_phase, "BUILD_READY", "应完成全部计划任务");
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
    })
    .unwrap();
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
    // 归档整合为一个人工文档和机器归档、完整性标记
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
    // 归档整合为一个人工文档，机器归档与报告统一保存在 runtime.json
    let runtime: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".sdd/runtime.json")).unwrap(),
    )
    .unwrap();
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let change_id = state.current_change_id.unwrap();
    assert!(runtime["changes"][change_id]["archive"].is_object());
    assert!(names.contains(&"archive.md".to_string()));
    assert!(!names.contains(&"revisions".to_string()));
    assert!(!names.contains(&"archive.json".to_string()));
    assert!(!names.contains(&".archived".to_string()));
    assert!(!names.contains(&"spec.md".to_string()));
    assert!(!names.contains(&"plan.md".to_string()));
    assert!(!names.contains(&"tasks.md".to_string()));
    let archive_markdown = std::fs::read_to_string(change_dir.join("archive.md")).unwrap();
    assert!(archive_markdown.contains("## 需求规格"));
    assert!(archive_markdown.contains("## 实施计划"));
    assert!(archive_markdown.contains("## 开发任务"));
}

#[test]
fn verify_rejects_tampered_spec_document() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    complete_all_tasks(dir.path(), &cwd);
    let change_dir = std::fs::read_dir(dir.path().join(".sdd/changes"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::OpenOptions::new()
        .append(true)
        .open(change_dir.join("spec.md"))
        .unwrap()
        .write_all(b"\ntampered")
        .unwrap();

    let err = run(&CommandRequest {
        command: "verify".into(),
        cwd,
        args: None,
    })
    .unwrap_err();
    assert_eq!(err.code, "E_COMPONENT_INTEGRITY_FAILED");
}

#[test]
fn review_rejects_tampered_tasks_document() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    complete_all_tasks(dir.path(), &cwd);
    run(&CommandRequest {
        command: "verify".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    let change_dir = std::fs::read_dir(dir.path().join(".sdd/changes"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::OpenOptions::new()
        .append(true)
        .open(change_dir.join("tasks.md"))
        .unwrap()
        .write_all(b"\ntampered")
        .unwrap();

    let err = run(&CommandRequest {
        command: "review".into(),
        cwd,
        args: None,
    })
    .unwrap_err();
    assert_eq!(err.code, "E_COMPONENT_INTEGRITY_FAILED");
}

#[test]
fn review_rejects_changes_made_after_verify() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@test.test"]);
    git(dir.path(), &["config", "user.name", "test"]);
    git(dir.path(), &["add", "README.md"]);
    git(dir.path(), &["commit", "-qm", "base"]);
    let cwd = prepare(dir.path());
    complete_all_tasks(dir.path(), &cwd);
    run(&CommandRequest {
        command: "verify".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let tasks = sdd_core::commands::plan::read_plan_tasks(
        &cwd,
        state.current_change_id.as_deref().unwrap(),
    )
    .unwrap();
    let changed = tasks
        .iter()
        .flat_map(|task| task.allowed_files.iter())
        .find(|path| !path.contains('*'))
        .unwrap();
    let changed_path = dir.path().join(changed);
    std::fs::create_dir_all(changed_path.parent().unwrap()).unwrap();
    std::fs::write(changed_path, "// changed").unwrap();
    let error = run(&CommandRequest {
        command: "review".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap_err();
    assert_eq!(error.code, "E_VERIFY_REQUIRED");
    assert_eq!(
        sdd_core::state::StateStore::new(cwd)
            .read()
            .unwrap()
            .current_phase,
        "BUILD_READY"
    );
}

#[test]
fn archive_rejects_failed_review_report() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    complete_all_tasks(dir.path(), &cwd);
    run(&CommandRequest {
        command: "verify".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    run(&CommandRequest {
        command: "review".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    let change_id = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap()
        .current_change_id
        .unwrap();
    let mut reports =
        sdd_core::state::runtime_store::read_change_field(&cwd, &change_id, "reports")
            .unwrap()
            .unwrap();
    reports["review"]["passed"] = json!(false);
    sdd_core::state::runtime_store::write_change_field(&cwd, &change_id, "reports", reports)
        .unwrap();
    let err = run(&CommandRequest {
        command: "archive".into(),
        cwd,
        args: None,
    })
    .unwrap_err();
    assert_eq!(err.code, "E_REVIEW_REQUIRED");
}

#[test]
fn archive_rejects_tampered_marker_on_retry() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = prepare(dir.path());
    complete_all_tasks(dir.path(), &cwd);
    for command in ["verify", "review", "archive"] {
        run(&CommandRequest {
            command: command.into(),
            cwd: cwd.clone(),
            args: None,
        })
        .unwrap();
    }
    let change_id = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap()
        .current_change_id
        .unwrap();
    let mut archive =
        sdd_core::state::runtime_store::read_change_field(&cwd, &change_id, "archive")
            .unwrap()
            .unwrap();
    archive["tampered"] = json!(true);
    sdd_core::state::runtime_store::write_change_field(&cwd, &change_id, "archive", archive)
        .unwrap();
    let err = run(&CommandRequest {
        command: "archive".into(),
        cwd,
        args: None,
    })
    .unwrap_err();
    assert_eq!(err.code, "E_COMPONENT_INTEGRITY_FAILED");
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

#[cfg(unix)]
fn prepare_ocr_fixture(
    content: &str,
    script_prefix: &str,
) -> (tempfile::TempDir, tempfile::TempDir, String, String) {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@test.test"]);
    git(dir.path(), &["config", "user.name", "test"]);
    git(dir.path(), &["add", "README.md"]);
    git(dir.path(), &["commit", "-qm", "base"]);

    let cwd = prepare(dir.path());
    complete_all_tasks(dir.path(), &cwd);
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let tasks = sdd_core::commands::plan::read_plan_tasks(
        &cwd,
        state.current_change_id.as_deref().unwrap(),
    )
    .unwrap();
    let changed = tasks
        .iter()
        .flat_map(|task| task.allowed_files.iter())
        .find(|path| path.ends_with(".rs") && !path.contains('*'))
        .cloned()
        .unwrap();
    let changed_path = dir.path().join(&changed);
    std::fs::create_dir_all(changed_path.parent().unwrap()).unwrap();
    std::fs::write(&changed_path, content).unwrap();

    let backend_dir = tempfile::tempdir().unwrap();
    let script = backend_dir.path().join("ocr");
    let output = json!({
        "status": "success",
        "session_id": "session-test",
        "comments": [{
            "path": changed,
            "content": "请处理错误",
            "start_line": 1,
            "end_line": 1,
            "category": "bug",
            "severity": "medium"
        }]
    })
    .to_string()
    .replace('\'', "'\\''");
    std::fs::write(
        &script,
        format!("#!/bin/sh\n{script_prefix}\nprintf '%s' '{output}'\n"),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let mut config = sdd_core::state::runtime_store::read_config(&cwd).unwrap();
    config["quality"]["ocr"] = json!({
        "mode": "auto",
        "command": script.to_string_lossy(),
    });
    sdd_core::state::runtime_store::write_config(&cwd, config).unwrap();
    run(&CommandRequest {
        command: "verify".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    (dir, backend_dir, cwd, changed)
}

#[cfg(unix)]
#[test]
fn review_merges_successful_ocr_findings() {
    let (_dir, _backend_dir, cwd, changed) = prepare_ocr_fixture("fn main() {}\n", "");
    let result = run(&CommandRequest {
        command: "review".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert!(result.ok);
    assert!(result.warnings.is_none());
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let report = sdd_core::state::runtime_store::read_change_field(
        &cwd,
        state.current_change_id.as_deref().unwrap(),
        "reports",
    )
    .unwrap()
    .unwrap()["review"]
        .clone();
    assert_eq!(report["minimality"]["ocr"]["status"], "completed");
    assert!(report["issues"].as_array().unwrap().iter().any(|issue| {
        issue["origin"] == "ocr"
            && issue["file"] == changed
            && issue["startLine"] == 1
            && issue["category"] == "bug"
    }));
}

#[cfg(unix)]
#[test]
fn blocking_ocr_finding_returns_review_failure_code() {
    let (_dir, backend_dir, cwd, _changed) = prepare_ocr_fixture("fn main() {}\n", "");
    let script = backend_dir.path().join("ocr");
    let script_text = std::fs::read_to_string(&script).unwrap();
    std::fs::write(&script, script_text.replace("\"medium\"", "\"critical\"")).unwrap();

    let error = run(&CommandRequest {
        command: "review".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap_err();
    assert_eq!(error.code, "E_REVIEW_FAILED");

    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let report = sdd_core::state::runtime_store::read_change_field(
        &cwd,
        state.current_change_id.as_deref().unwrap(),
        "reports",
    )
    .unwrap()
    .unwrap()["review"]
        .clone();
    assert!(report["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == "OCR_FINDING"));
}

#[cfg(unix)]
#[test]
fn review_auto_mode_warns_and_keeps_deterministic_result_when_ocr_missing() {
    let (_dir, _backend_dir, cwd, _changed) = prepare_ocr_fixture("fn main() {}\n", "");
    let mut config = sdd_core::state::runtime_store::read_config(&cwd).unwrap();
    config["quality"]["ocr"]["command"] = json!("/definitely/missing/ocr");
    sdd_core::state::runtime_store::write_config(&cwd, config).unwrap();
    let result = run(&CommandRequest {
        command: "review".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert_eq!(result.state, "REVIEW_READY");
    assert_eq!(
        result.warnings.as_ref().unwrap()[0]["code"],
        "W_OCR_NOT_FOUND"
    );
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let report = sdd_core::state::runtime_store::read_change_field(
        &cwd,
        &state.current_change_id.clone().unwrap(),
        "reports",
    )
    .unwrap()
    .unwrap()["review"]
        .clone();
    assert_eq!(report["passed"], true);
    assert_eq!(report["minimality"]["ocr"]["status"], "not-found");
}

#[cfg(unix)]
#[test]
fn deterministic_blocker_prevents_ocr_process_start() {
    let marker_dir = tempfile::tempdir().unwrap();
    let marker = marker_dir.path().join("started");
    let (_dir, backend_dir, cwd, _changed) = prepare_ocr_fixture(
        "aws_access_key_id=AKIAIOSFODNN7EXAMPLE\n",
        &format!("touch '{}'", marker.display()),
    );
    let result = run(&CommandRequest {
        command: "review".into(),
        cwd,
        args: None,
    });
    assert_eq!(result.unwrap_err().code, "E_SECURITY_BLOCKED");
    assert!(!marker.exists());
    drop(backend_dir);
}

#[cfg(unix)]
#[test]
fn started_ocr_failure_is_hard_failure_and_persists_report() {
    let (_dir, _backend_dir, cwd, _changed) = prepare_ocr_fixture("fn main() {}\n", "exit 7");
    let error = run(&CommandRequest {
        command: "review".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap_err();
    assert_eq!(error.code, "E_REVIEW_BACKEND_FAILED");
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    assert_eq!(state.current_phase, "VERIFY_READY");
    let report = sdd_core::state::runtime_store::read_change_field(
        &cwd,
        state.current_change_id.as_deref().unwrap(),
        "reports",
    )
    .unwrap()
    .unwrap()["review"]
        .clone();
    assert_eq!(report["passed"], false);
    assert_eq!(report["minimality"]["ocr"]["status"], "failed");
}

#[cfg(unix)]
#[test]
fn required_ocr_mode_errors_when_command_is_missing() {
    let (_dir, _backend_dir, cwd, _changed) = prepare_ocr_fixture("fn main() {}\n", "");
    let mut config = sdd_core::state::runtime_store::read_config(&cwd).unwrap();
    config["quality"]["ocr"] = json!({
        "mode": "required",
        "command": "/definitely/missing/ocr"
    });
    sdd_core::state::runtime_store::write_config(&cwd, config).unwrap();
    let error = run(&CommandRequest {
        command: "review".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap_err();
    assert_eq!(error.code, "E_REVIEW_BACKEND_UNAVAILABLE");
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let report = sdd_core::state::runtime_store::read_change_field(
        &cwd,
        &state.current_change_id.clone().unwrap(),
        "reports",
    )
    .unwrap()
    .unwrap()["review"]
        .clone();
    assert_eq!(report["passed"], false);
    assert_eq!(report["minimality"]["ocr"]["status"], "unavailable");
}

#[cfg(unix)]
#[test]
fn review_off_mode_does_not_run_ocr_or_warn() {
    let (_dir, _backend_dir, cwd, _changed) = prepare_ocr_fixture("fn main() {}\n", "");
    let mut config = sdd_core::state::runtime_store::read_config(&cwd).unwrap();
    config["quality"]["ocr"]["mode"] = json!("off");
    sdd_core::state::runtime_store::write_config(&cwd, config).unwrap();
    let result = run(&CommandRequest {
        command: "review".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();
    assert_eq!(result.state, "REVIEW_READY");
    assert!(result.ok);
    assert!(result.warnings.is_none(), "off 模式不应产生 OCR 警告");
    let state = sdd_core::state::StateStore::new(cwd.clone())
        .read()
        .unwrap();
    let report = sdd_core::state::runtime_store::read_change_field(
        &cwd,
        &state.current_change_id.clone().unwrap(),
        "reports",
    )
    .unwrap()
    .unwrap()["review"]
        .clone();
    assert_eq!(report["minimality"]["ocr"]["status"], "off");
    assert_eq!(report["passed"], true);
}
