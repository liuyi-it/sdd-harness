//! 状态存储与文件锁测试。

use sdd_core::state::{lock_sdd, RuntimeStore, StateStore};

fn initialize_runtime(cwd: &str) {
    RuntimeStore::new(cwd.to_string())
        .update(|runtime| {
            runtime.state.current_phase = "INDEX_READY".to_string();
            runtime.state.initialized = true;
            runtime.state.index_status = "UNAVAILABLE".to_string();
            runtime.state.codebase_provider = "fallback-file-scan".to_string();
            runtime.state.degraded = true;
            runtime.state.degraded_reason = Some("测试环境未提供 CodeGraph".to_string());
            runtime.state.last_command = Some("first-command".to_string());
            runtime.index = serde_json::json!({
                "diagnostics": [{
                    "provider": "codegraph",
                    "installed": false,
                    "version": null,
                    "indexed": false,
                    "degraded": true,
                    "reason": "测试环境未提供 CodeGraph"
                }],
                "summary": "<!-- summary-provider: fallback-file-scan degraded=true -->\n测试索引摘要",
                "updatedAt": "2026-01-01T00:00:00Z"
            });
        })
        .unwrap();
}

#[test]
fn read_missing_state_returns_not_initialized() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    let state = store.read().unwrap();
    assert_eq!(state.current_phase, "NOT_INITIALIZED");
    assert!(!state.initialized);
}

#[test]
fn update_then_read_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let store = StateStore::new(cwd.clone());
    initialize_runtime(&cwd);
    let read = store.read().unwrap();
    assert_eq!(read.current_phase, "INDEX_READY");
    assert!(read.initialized);
}

#[test]
fn lock_is_exclusive() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let _guard = lock_sdd(&cwd, "sdd test", None, None).unwrap();
    // 其他线程获取必须失败；同线程嵌套获取由组合命令复用同一个 OS 锁。
    let err = std::thread::spawn(move || lock_sdd(&cwd, "sdd competing", None, None).unwrap_err())
        .join()
        .unwrap();
    assert_eq!(err.code, "E_CONCURRENT_RUN");
    assert!(err.message.contains("sdd test"));
}

#[test]
fn lock_times_out_when_held_with_short_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let _guard = lock_sdd(&cwd, "sdd test", None, None).unwrap();
    // 其他线程以小 timeout_ms 等待持有锁 → E_LOCK_TIMEOUT
    let err =
        std::thread::spawn(move || lock_sdd(&cwd, "sdd competing", None, Some(50)).unwrap_err())
            .join()
            .unwrap();
    assert_eq!(err.code, "E_LOCK_TIMEOUT");
}

#[test]
fn lock_is_reentrant_on_same_thread_and_remains_exclusive() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let first = lock_sdd(&cwd, "sdd outer", None, None).unwrap();
    let nested = lock_sdd(&cwd, "sdd nested", None, None).unwrap();

    // 即使 outer 先释放，nested 仍必须继续持有底层文件描述符。
    drop(first);
    let competing_cwd = cwd.clone();
    let err = std::thread::spawn(move || {
        lock_sdd(&competing_cwd, "sdd competing", None, None).unwrap_err()
    })
    .join()
    .unwrap();
    assert_eq!(err.code, "E_CONCURRENT_RUN");

    drop(nested);
    let _guard = lock_sdd(&cwd, "sdd replacement", None, None).unwrap();
}

#[cfg(unix)]
#[test]
fn lock_is_reentrant_across_paths_to_the_same_project() {
    let parent = tempfile::tempdir().unwrap();
    let project = parent.path().join("project");
    let alias = parent.path().join("project-alias");
    std::fs::create_dir(&project).unwrap();
    std::os::unix::fs::symlink(&project, &alias).unwrap();

    let project = project.to_string_lossy().to_string();
    let alias = alias.to_string_lossy().to_string();
    let first = lock_sdd(&project, "sdd outer", None, None).unwrap();
    let nested = lock_sdd(&alias, "sdd nested", None, None).unwrap();

    drop(first);
    drop(nested);
    let _guard = lock_sdd(&project, "sdd replacement", None, None).unwrap();
}

#[test]
fn lock_metadata_identifies_the_current_holder() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let _guard = lock_sdd(&cwd, "sdd test", Some("change-a"), None).unwrap();
    let lock_path = dir.path().join(".sdd/lock.owner.json");
    let metadata: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
    assert_eq!(metadata["command"], "sdd test");
    assert_eq!(metadata["change_id"], "change-a");
    assert!(metadata["pid"].as_u64().is_some());
}

#[test]
fn lock_releases_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    {
        let _guard = lock_sdd(&cwd, "sdd test", None, None).unwrap();
    }
    // 锁哨兵和诊断信息保留，但文件描述符释放后可再次获取。
    assert!(dir.path().join(".sdd/lock").exists());
    assert!(dir.path().join(".sdd/lock.owner.json").exists());
    let _guard = lock_sdd(&cwd, "sdd test", None, None).unwrap();
}

#[test]
fn orphaned_lock_metadata_does_not_block_new_owner() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    std::fs::create_dir_all(dir.path().join(".sdd")).unwrap();
    let path = dir.path().join(".sdd/lock.owner.json");
    std::fs::write(
        &path,
        r#"{"pid":999999,"command":"abandoned command","created_at":"2000-01-01T00:00:00Z"}"#,
    )
    .unwrap();
    let _guard = lock_sdd(&cwd, "sdd replacement", None, None).unwrap();
    let metadata: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(metadata["command"], "sdd replacement");
    assert!(path.exists());
}

#[test]
fn update_modifies_and_bumps_version() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let store = StateStore::new(cwd.clone());
    initialize_runtime(&cwd);
    let state = store
        .update(|s| s.last_command = Some("second".into()))
        .unwrap();
    assert_eq!(state.current_phase, "INDEX_READY");
    assert!(state.version >= 2);
}

#[test]
fn update_rejects_version_overflow() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    RuntimeStore::new(dir.path().to_string_lossy().to_string())
        .update(|runtime| runtime.state.version = u32::MAX)
        .unwrap();

    let error = store.update(|_| {}).unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[test]
fn corrupted_state_file_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".sdd")).unwrap();
    std::fs::write(dir.path().join(".sdd/runtime.json"), "{not json").unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    let err = store.read().unwrap_err();
    assert_eq!(err.code, "E_STATE_CORRUPTED");
}

#[test]
fn corrupted_primary_recovers_from_verified_backup() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let store = StateStore::new(cwd.clone());
    initialize_runtime(&cwd);
    store
        .update(|state| state.last_command = Some("second-command".to_string()))
        .unwrap();

    assert!(dir.path().join(".sdd/runtime.json.bak.sha256").exists());
    std::fs::write(store.state_path(), "{broken").unwrap();
    let recovered = store.read().unwrap();
    assert_eq!(recovered.current_phase, "INDEX_READY");
    assert_eq!(recovered.last_command.as_deref(), Some("first-command"));
}

#[test]
fn checksum_mismatch_is_detected_and_falls_back_to_backup() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let store = StateStore::new(cwd.clone());
    initialize_runtime(&cwd);
    store
        .update(|state| state.last_command = Some("second-command".to_string()))
        .unwrap();
    // 主文件 + 校验和已由 write 写入；篡改主文件内容（保持 JSON 合法）→ 校验失败
    let path = store.state_path();
    let mut content = std::fs::read_to_string(&path).unwrap();
    content = content.replacen("second-command", "tampered-command", 1);
    assert_ne!(content, std::fs::read_to_string(&path).unwrap());
    std::fs::write(&path, content).unwrap();
    // read 应回退到 bak（INDEX_READY 快照）
    let recovered = store.read().unwrap();
    assert_eq!(recovered.current_phase, "INDEX_READY");
    assert_eq!(recovered.last_command.as_deref(), Some("first-command"));
    // 下一轮 write 会重建校验和。
    store
        .update(|s| s.last_command = Some("third-command".to_string()))
        .unwrap();
    let again = store.read().unwrap();
    assert_eq!(again.last_command.as_deref(), Some("third-command"));
}

#[test]
fn read_without_checksum_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    store.update(|_| {}).unwrap();
    std::fs::remove_file(dir.path().join(".sdd/runtime.json.sha256")).unwrap();

    let error = store.read().unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[test]
fn invalid_phase_is_rejected_before_write() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    let err = store
        .update(|state| state.current_phase = "NOT_A_PHASE".to_string())
        .unwrap_err();
    assert_eq!(err.code, "E_STATE_CORRUPTED");
}

#[test]
fn unsafe_persisted_ids_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    let err = store
        .update(|state| state.current_run_id = Some("../escape".to_string()))
        .unwrap_err();
    assert_eq!(err.code, "E_SECURITY_BLOCKED");
}

#[test]
fn phase_requiring_change_rejects_missing_aggregate_ids() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());

    let error = store
        .update(|state| state.current_phase = "NEW_STARTED".to_string())
        .unwrap_err();

    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[test]
fn failure_command_and_reason_must_be_persisted_together() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());

    let error = store
        .update(|state| state.failed_command = Some("sdd verify".to_string()))
        .unwrap_err();

    assert_eq!(error.code, "E_STATE_CORRUPTED");
}

#[test]
fn task_state_rejects_invalid_ids_and_orphaned_building_status() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    initialize_runtime(&cwd);
    let store = StateStore::new(cwd);

    let error = store
        .update(|state| {
            state
                .tasks
                .insert("task-invalid".to_string(), "PENDING".to_string());
        })
        .unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");

    let error = store
        .update(|state| {
            state
                .tasks
                .insert("TASK-001-RED".to_string(), "BUILDING".to_string());
        })
        .unwrap_err();
    assert_eq!(error.code, "E_STATE_CORRUPTED");
}
