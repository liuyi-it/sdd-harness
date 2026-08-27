use sdd_core::contracts::CommandRequest;
use sdd_core::run;
use serde_json::Value;

#[test]
fn init_persists_all_machine_data_in_runtime_json() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();

    let result = run(&CommandRequest {
        command: "init".into(),
        cwd,
        args: None,
    })
    .unwrap();
    assert_eq!(result.state, "INDEX_READY");

    let runtime_path = dir.path().join(".sdd/runtime.json");
    assert!(runtime_path.exists());
    assert!(dir.path().join(".sdd/runtime.json.sha256").exists());
    assert!(!dir.path().join(".sdd/runtime.json.hmac").exists());
    let runtime: Value =
        serde_json::from_str(&std::fs::read_to_string(runtime_path).unwrap()).unwrap();
    assert!(sdd_core::schema::validate_json("runtime", &runtime).is_ok());
    for key in [
        "schemaVersion",
        "state",
        "config",
        "artifacts",
        "changes",
        "runs",
        "loop",
        "index",
    ] {
        assert!(runtime.get(key).is_some(), "runtime 缺少 {key}");
    }
    for path in [
        ".sdd/state.json",
        ".sdd/state.json.bak",
        ".sdd/config.json",
        ".sdd/artifacts.json",
        ".sdd/index/knowledge.json",
        ".sdd/index/summary.md",
    ] {
        assert!(!dir.path().join(path).exists(), "不应生成 {path}");
    }
}

#[test]
fn batched_field_writes_persist_related_runtime_data_together() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();

    sdd_core::state::RuntimeStore::new(cwd.clone())
        .update(|runtime| {
            runtime.runs.insert(
                "run-1".to_string(),
                serde_json::json!({
                    "changeId": "change-1",
                    "input": "实现批量写入",
                    "answers": { "Q-GOAL": "验证" }
                }),
            );
            runtime.changes.insert(
                "change-1".to_string(),
                serde_json::json!({
                    "spec": {
                        "schemaVersion": "3.0.0",
                        "status": "READY",
                        "requirement": "实现批量写入",
                        "impact": "# 影响分析",
                        "answers": { "Q-GOAL": "验证" },
                        "model": {
                            "requirements": [{
                                "id": "REQ-001",
                                "title": "成功行为 1",
                                "statement": "批量写入保持原子性",
                                "scenarios": [{
                                    "id": "REQ-001-SC-001",
                                    "title": "批量写入成功",
                                    "given": ["输入有效"],
                                    "when": ["执行批量写入"],
                                    "then": ["全部数据写入成功"]
                                }]
                            }]
                        }
                    },
                    "design": "设计"
                }),
            );
        })
        .unwrap();

    let runtime = sdd_core::state::RuntimeStore::new(cwd).read().unwrap();
    assert_eq!(runtime.runs["run-1"]["input"], "实现批量写入");
    assert_eq!(runtime.runs["run-1"]["changeId"], "change-1");
    assert_eq!(runtime.runs["run-1"]["answers"]["Q-GOAL"], "验证");
    assert_eq!(runtime.changes["change-1"]["spec"]["status"], "READY");
    assert_eq!(runtime.changes["change-1"]["design"], "设计");
}

#[test]
fn runtime_rejects_orphaned_or_mismatched_business_runs() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let error = sdd_core::state::RuntimeStore::new(cwd.clone())
        .update(|runtime| {
            runtime.runs.insert(
                "run-orphan".to_string(),
                serde_json::json!({
                    "changeId": "missing-change",
                    "input": "需求",
                    "answers": {}
                }),
            );
        })
        .unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");

    let error = sdd_core::state::RuntimeStore::new(cwd)
        .update(|runtime| {
            runtime
                .changes
                .insert("change-a".to_string(), serde_json::json!({}));
            runtime
                .changes
                .insert("change-b".to_string(), serde_json::json!({}));
            runtime.runs.insert(
                "run-a".to_string(),
                serde_json::json!({
                    "changeId": "change-a",
                    "input": "需求",
                    "answers": {}
                }),
            );
            runtime.state.initialized = true;
            runtime.state.current_phase = "NEW_STARTED".to_string();
            runtime.state.current_change_id = Some("change-b".to_string());
            runtime.state.current_run_id = Some("run-a".to_string());
            runtime.state.index_status = "UNAVAILABLE".to_string();
            runtime.state.codebase_provider = "fallback-file-scan".to_string();
            runtime.state.degraded = true;
            runtime.state.degraded_reason = Some("测试".to_string());
            runtime.index = serde_json::json!({
                "diagnostics": [{
                    "provider": "codegraph",
                    "installed": false,
                    "version": null,
                    "indexed": false,
                    "degraded": true,
                    "reason": "测试"
                }],
                "summary": "<!-- summary-provider: fallback-file-scan degraded=true -->\n测试",
                "updatedAt": "2026-01-01T00:00:00Z"
            });
        })
        .unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[test]
fn change_runtime_fields_reject_unsafe_change_ids() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();

    let error = sdd_core::state::RuntimeStore::new(cwd)
        .update(|runtime| {
            runtime
                .changes
                .insert("../outside".to_string(), serde_json::json!({}));
        })
        .unwrap_err();

    assert_eq!(error.code, "E_SECURITY_BLOCKED");
}

#[test]
fn runtime_rejects_malformed_artifact_registry() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();

    let error = sdd_core::state::RuntimeStore::new(cwd)
        .update(|runtime| {
            runtime.artifacts = serde_json::json!({ "artifacts": [] });
        })
        .unwrap_err();

    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[test]
fn runtime_rejects_index_shape_inconsistent_with_state() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();

    let error = sdd_core::state::RuntimeStore::new(cwd)
        .update(|runtime| {
            runtime.state.index_status = "INDEX_READY".to_string();
            runtime.index = serde_json::json!({ "summary": "missing diagnostics" });
        })
        .unwrap_err();

    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[test]
fn runtime_rejects_summary_metadata_inconsistent_with_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();

    let runtime = sdd_core::state::RuntimeStore::new(cwd.clone())
        .read()
        .unwrap();
    let conflicting_prefix = if runtime.state.degraded {
        "<!-- summary-provider: codegraph degraded=false -->"
    } else {
        "<!-- summary-provider: fallback-file-scan degraded=true -->"
    };
    let error = sdd_core::state::RuntimeStore::new(cwd)
        .update(|runtime| {
            runtime.index["summary"] =
                serde_json::json!(format!("{conflicting_prefix}\nwrong provider"));
        })
        .unwrap_err();

    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[test]
fn runtime_rejects_indexed_codegraph_marked_as_not_installed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();

    let error = sdd_core::state::RuntimeStore::new(cwd)
        .update(|runtime| {
            runtime.state.index_status = "INDEX_READY".to_string();
            runtime.state.codebase_provider = "codegraph".to_string();
            runtime.state.degraded = false;
            runtime.state.degraded_reason = None;
            runtime.index["diagnostics"][0] = serde_json::json!({
                "provider": "codegraph",
                "installed": false,
                "version": null,
                "indexed": true,
                "degraded": false,
                "reason": null
            });
            runtime.index["summary"] =
                serde_json::json!("<!-- summary-provider: codegraph degraded=false -->\nsummary");
        })
        .unwrap_err();

    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[test]
fn runtime_rejects_degradation_reason_inconsistent_with_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    run(&CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: None,
    })
    .unwrap();

    let error = sdd_core::state::RuntimeStore::new(cwd)
        .update(|runtime| {
            runtime.state.index_status = "UNAVAILABLE".to_string();
            runtime.state.codebase_provider = "fallback-file-scan".to_string();
            runtime.state.degraded = true;
            runtime.state.degraded_reason = Some("state reason".to_string());
            runtime.index["diagnostics"][0]["indexed"] = serde_json::json!(false);
            runtime.index["diagnostics"][0]["degraded"] = serde_json::json!(true);
            runtime.index["diagnostics"][0]["reason"] = serde_json::json!("diagnostic reason");
            runtime.index["summary"] = serde_json::json!(
                "<!-- summary-provider: fallback-file-scan degraded=true -->\nsummary"
            );
        })
        .unwrap_err();

    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[test]
fn runtime_rejects_loop_events_without_run_record() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();

    let error = sdd_core::state::RuntimeStore::new(cwd)
        .update(|runtime| {
            runtime.loop_state["events"]["run-1"] = serde_json::json!([]);
        })
        .unwrap_err();

    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[cfg(unix)]
#[test]
fn runtime_rejects_symlinked_state_directory() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), project.path().join(".sdd")).unwrap();

    let error = sdd_core::state::RuntimeStore::new(project.path().to_string_lossy().to_string())
        .update(|_| {})
        .unwrap_err();

    assert_eq!(error.code, "E_SYMLINK_BLOCKED");
    assert!(!outside.path().join("runtime.json").exists());
}

#[cfg(unix)]
#[test]
fn runtime_rejects_symlinked_runtime_file() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::create_dir(project.path().join(".sdd")).unwrap();
    std::os::unix::fs::symlink(outside.path(), project.path().join(".sdd/runtime.json")).unwrap();

    let error = sdd_core::state::RuntimeStore::new(project.path().to_string_lossy().to_string())
        .update(|_| {})
        .unwrap_err();

    assert_eq!(error.code, "E_SYMLINK_BLOCKED");
    assert_eq!(std::fs::read(outside.path()).unwrap(), Vec::<u8>::new());
}
