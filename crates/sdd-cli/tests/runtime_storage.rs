//! 以真实 CLI 进程验证初始化文件布局、损坏拒绝及操作系统锁释放。

use std::fs;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sdd"))
        .current_dir(root)
        .args(args)
        .arg("--json")
        // 本组只验证持久化，不依赖本机安装的可选 CodeGraph。
        .env("PATH", "")
        .output()
        .unwrap()
}

fn initialize(root: &Path) {
    let output = cli(root, &["init"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["state"],
        "INDEX_READY"
    );
}

fn entries(root: &Path) -> Vec<String> {
    let mut names = fs::read_dir(root.join(".sdd"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn init_and_reinit_leave_only_runtime_and_lock() {
    let dir = tempfile::tempdir().unwrap();
    for _ in 0..2 {
        initialize(dir.path());
        assert_eq!(entries(dir.path()), ["lock", "runtime.json"]);
        assert!(cli(dir.path(), &["status"]).status.success());
    }
}

#[test]
fn corrupted_and_old_states_are_rejected_without_reinitialization() {
    let dir = tempfile::tempdir().unwrap();
    initialize(dir.path());
    let path = dir.path().join(".sdd/runtime.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["config"]["audit"]["maxFiles"] = json!(201);
    for (bytes, expected) in [
        (serde_json::to_vec(&value).unwrap(), "E_STATE_CORRUPTED"),
        (
            b"{\"schemaVersion\":7}".to_vec(),
            "E_STATE_VERSION_UNSUPPORTED",
        ),
    ] {
        fs::write(&path, &bytes).unwrap();
        for command in ["status", "init"] {
            let output = cli(dir.path(), &[command]);
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stdout).contains(expected));
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(entries(dir.path()), ["lock", "runtime.json"]);
        }
    }
}

// 即使父测试断言失败也回收子进程，避免测试本身留下锁占用。
struct LockProcess(Child);

impl Drop for LockProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn lock_holder_process() {
    let Some(root) = std::env::var_os("SDD_TEST_LOCK_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let _guard = sdd_core::state::file_lock::lock_sdd(root.to_str().unwrap(), None).unwrap();
    fs::write(root.join("lock-ready"), "ready").unwrap();
    // 父测试会主动终止本进程；上界同时避免独立误调用永久等待。
    std::thread::sleep(Duration::from_secs(30));
}

#[test]
fn another_process_cannot_write_until_the_lock_holder_exits() {
    let dir = tempfile::tempdir().unwrap();
    initialize(dir.path());
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_holder_process", "--nocapture"])
        .env("SDD_TEST_LOCK_ROOT", dir.path())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut holder = LockProcess(child);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !dir.path().join("lock-ready").exists() {
        assert!(holder.0.try_wait().unwrap().is_none(), "持锁进程提前退出");
        assert!(Instant::now() < deadline, "持锁进程未在期限内就绪");
        std::thread::sleep(Duration::from_millis(10));
    }
    let original = fs::read(dir.path().join(".sdd/runtime.json")).unwrap();
    let blocked = cli(dir.path(), &["init"]);
    assert!(!blocked.status.success());
    assert!(String::from_utf8_lossy(&blocked.stdout).contains("E_CONCURRENT_RUN"));
    assert_eq!(
        fs::read(dir.path().join(".sdd/runtime.json")).unwrap(),
        original
    );
    assert_eq!(entries(dir.path()), ["lock", "runtime.json"]);

    holder.0.kill().unwrap();
    holder.0.wait().unwrap();
    initialize(dir.path());
    assert_eq!(entries(dir.path()), ["lock", "runtime.json"]);
}
