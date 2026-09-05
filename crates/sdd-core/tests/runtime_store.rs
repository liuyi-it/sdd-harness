//! 通过公开存储接口验证双文件持久化、完整性和事务边界。

use std::fs;
use std::path::Path;

use sdd_core::error::SddError;
use sdd_core::state::file_lock::lock_sdd;
use sdd_core::state::{checksum, RuntimeStore};
use serde_json::{json, Value};

fn entries(root: &Path) -> Vec<String> {
    let mut names = fs::read_dir(root.join(".sdd"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn stored_value(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join(".sdd/runtime.json")).unwrap()).unwrap()
}

#[test]
fn reading_an_uninitialized_project_does_not_create_files() {
    let dir = tempfile::tempdir().unwrap();
    let state = RuntimeStore::new(dir.path()).read().unwrap();
    assert!(!state.state.initialized);
    assert!(!dir.path().join(".sdd").exists());
}

#[test]
fn repeated_transactions_keep_two_files_and_embed_the_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::new(dir.path());
    store.update(|_| {}).unwrap();
    store
        .update(|document| document.config["audit"]["maxFiles"] = json!(201))
        .unwrap();

    assert_eq!(entries(dir.path()), ["lock", "runtime.json"]);
    let mut value = stored_value(dir.path());
    assert_eq!(value["schemaVersion"], 8);
    let expected = value.as_object_mut().unwrap().remove("checksum").unwrap();
    assert_eq!(
        expected,
        checksum::compute(&serde_json::to_vec(&value).unwrap())
    );
    assert_eq!(store.read().unwrap().config["audit"]["maxFiles"], 201);
}

#[test]
fn whitespace_and_object_key_order_do_not_change_integrity() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::new(dir.path());
    store.update(|_| {}).unwrap();
    let value = stored_value(dir.path());
    let reversed = value
        .as_object()
        .unwrap()
        .iter()
        .rev()
        .map(|(key, value)| {
            format!(
                "  {}: {}",
                serde_json::to_string(key).unwrap(),
                serde_json::to_string_pretty(value).unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    fs::write(
        dir.path().join(".sdd/runtime.json"),
        format!("{{\n{reversed}\n}}\n"),
    )
    .unwrap();

    assert!(store.read().is_ok());
}

#[test]
fn corruption_never_recovers_from_a_backup_or_overwrites_the_primary() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::new(dir.path());
    store.update(|_| {}).unwrap();
    let path = dir.path().join(".sdd/runtime.json");
    let original = fs::read(&path).unwrap();
    // 故意放置旧恢复文件，证明新存储层不会悄悄采用它们。
    fs::write(dir.path().join(".sdd/runtime.json.bak"), &original).unwrap();
    fs::write(
        dir.path().join(".sdd/runtime.json.bak.sha256"),
        checksum::compute(&original),
    )
    .unwrap();
    let value: Value = serde_json::from_slice(&original).unwrap();
    let mut changed = value.clone();
    changed["config"]["audit"]["maxFiles"] = json!(201);
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("checksum");
    let mut invalid = value.clone();
    invalid["checksum"] = json!("0".repeat(64));
    let mut wrong_type = value;
    wrong_type["checksum"] = json!(7);

    for raw in [
        serde_json::to_vec(&changed).unwrap(),
        serde_json::to_vec(&missing).unwrap(),
        serde_json::to_vec(&invalid).unwrap(),
        serde_json::to_vec(&wrong_type).unwrap(),
        b"{incomplete".to_vec(),
    ] {
        fs::write(&path, &raw).unwrap();
        assert_eq!(store.read().unwrap_err().code, "E_STATE_CORRUPTED");
        assert_eq!(
            store
                .update(|_| panic!("损坏状态不得进入更新闭包"))
                .unwrap_err()
                .code,
            "E_STATE_CORRUPTED"
        );
        assert_eq!(fs::read(&path).unwrap(), raw);
        assert_eq!(
            fs::read(dir.path().join(".sdd/runtime.json.bak")).unwrap(),
            original
        );
    }
}

#[test]
fn rejected_transaction_preserves_the_last_complete_document() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::new(dir.path());
    store.update(|_| {}).unwrap();
    let path = dir.path().join(".sdd/runtime.json");
    let original = fs::read(&path).unwrap();
    let error = store
        .try_update(|document| {
            document.config["audit"]["maxFiles"] = json!(201);
            Err::<(), _>(SddError::new("E_INVALID_PHASE_COMMAND", "拒绝本次更新"))
        })
        .unwrap_err();

    assert_eq!(error.code, "E_INVALID_PHASE_COMMAND");
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(entries(dir.path()), ["lock", "runtime.json"]);
    assert_eq!(store.read().unwrap().config["audit"]["maxFiles"], 200);
}

#[test]
fn nested_lock_lives_until_the_last_guard_and_reports_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let outer = lock_sdd(root, None).unwrap();
    let inner = lock_sdd(root, None).unwrap();
    RuntimeStore::new(dir.path()).update(|_| {}).unwrap();
    drop(outer);

    let path = dir.path().to_owned();
    std::thread::spawn(move || {
        let root = path.to_str().unwrap();
        assert_eq!(lock_sdd(root, None).unwrap_err().code, "E_CONCURRENT_RUN");
        assert_eq!(lock_sdd(root, Some(20)).unwrap_err().code, "E_LOCK_TIMEOUT");
    })
    .join()
    .unwrap();

    drop(inner);
    let path = dir.path().to_owned();
    std::thread::spawn(move || lock_sdd(path.to_str().unwrap(), None).map(drop))
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(entries(dir.path()), ["lock", "runtime.json"]);
}

#[test]
fn concurrent_transactions_preserve_all_updates_and_complete_reads() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::new(dir.path());
    store.update(|_| {}).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let mut threads = Vec::new();
    for _ in 0..4 {
        let root = dir.path().to_owned();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let store = RuntimeStore::new(root);
            barrier.wait();
            for _ in 0..10 {
                store
                    .update(|document| {
                        let count = document.config["audit"]["maxFiles"].as_u64().unwrap();
                        document.config["audit"]["maxFiles"] = json!(count + 1);
                    })
                    .unwrap();
                // 读操作不持写锁，也必须看到完整且校验一致的一版状态。
                store.read().unwrap();
            }
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(store.read().unwrap().config["audit"]["maxFiles"], 240);
    assert_eq!(entries(dir.path()), ["lock", "runtime.json"]);
}

#[cfg(unix)]
#[test]
fn symlinked_runtime_cannot_read_or_overwrite_an_external_file() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let store = RuntimeStore::new(dir.path());
    store.update(|_| {}).unwrap();
    let path = dir.path().join(".sdd/runtime.json");
    let external = outside.path().join("runtime.json");
    let original = fs::read(&path).unwrap();
    fs::write(&external, &original).unwrap();
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&external, &path).unwrap();

    assert!(store.read().is_err());
    assert!(store.update(|_| {}).is_err());
    assert_eq!(fs::read(&external).unwrap(), original);
}
