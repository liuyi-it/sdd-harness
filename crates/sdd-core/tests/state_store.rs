//! 状态存储与文件锁测试。

use sdd_core::state::{lock_sdd, StateStore, WorkflowState};

#[test]
fn read_missing_state_returns_not_initialized() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    let state = store.read().unwrap();
    assert_eq!(state.current_phase, "NOT_INITIALIZED");
    assert!(!state.initialized);
}

#[test]
fn write_then_read_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    let mut state = WorkflowState::not_initialized();
    state.current_phase = "INDEX_READY".to_string();
    state.initialized = true;
    store.write(&state).unwrap();
    let read = store.read().unwrap();
    assert_eq!(read.current_phase, "INDEX_READY");
    assert!(read.initialized);
}

#[test]
fn lock_is_exclusive() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let _guard = lock_sdd(&cwd, "sdd test", None, None).unwrap();
    // 第二次获取（未过期、进程存活）必须失败：E_CONCURRENT_RUN
    let err = lock_sdd(&cwd, "sdd test", None, None).unwrap_err();
    assert_eq!(err.code, "E_CONCURRENT_RUN");
}

#[test]
fn lock_releases_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    {
        let _guard = lock_sdd(&cwd, "sdd test", None, None).unwrap();
    }
    // 释放后可再次获取
    let _guard = lock_sdd(&cwd, "sdd test", None, None).unwrap();
}

#[test]
fn old_guard_does_not_remove_replaced_lock() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let guard = lock_sdd(&cwd, "sdd old", None, None).unwrap();
    let path = dir.path().join(".sdd/lock");
    std::fs::write(
        &path,
        r#"{"pid":999999,"command":"sdd replacement","created_at":"2999-01-01T00:00:00Z","expires_at":"2999-01-01T00:10:00Z"}"#,
    )
    .unwrap();
    drop(guard);
    assert!(path.exists());
}

#[test]
fn update_modifies_and_bumps_version() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    let state = store
        .update(|s| {
            s.current_phase = "SPEC_READY".to_string();
        })
        .unwrap();
    assert_eq!(state.current_phase, "SPEC_READY");
    assert!(state.version >= 2);
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
fn corrupted_primary_recovers_from_backup() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    let mut first = WorkflowState::not_initialized();
    first.current_phase = "INDEX_READY".to_string();
    first.initialized = true;
    store.write(&first).unwrap();
    let mut second = first.clone();
    second.current_phase = "SPEC_READY".to_string();
    store.write(&second).unwrap();

    std::fs::write(store.state_path(), "{broken").unwrap();
    let recovered = store.read().unwrap();
    assert_eq!(recovered.current_phase, "INDEX_READY");
}

#[test]
fn invalid_phase_is_rejected_before_write() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    let mut state = WorkflowState::not_initialized();
    state.current_phase = "NOT_A_PHASE".to_string();
    let err = store.write(&state).unwrap_err();
    assert_eq!(err.code, "E_STATE_CORRUPTED");
}

#[test]
fn unsafe_persisted_ids_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = StateStore::new(dir.path().to_string_lossy().to_string());
    let mut state = WorkflowState::not_initialized();
    state.current_run_id = Some("../escape".to_string());
    let err = store.write(&state).unwrap_err();
    assert_eq!(err.code, "E_SECURITY_BLOCKED");
}
