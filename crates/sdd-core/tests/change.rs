//! sdd change 命令的端到端契约测试。

use sdd_core::contracts::CommandRequest;
use sdd_core::run;
use serde_json::json;

const INITIAL_REQUIREMENT: &str =
    "授权用户通过 POST /orders/{id}/cancel 请求取消待处理订单，入参 order_id，返回 status 和 error_code，未授权请求被拒绝，返回取消成功，每次取消写审计日志，需要自动化测试覆盖成功与未授权";
const REVISED_REQUIREMENT: &str =
    "授权用户通过 PATCH /orders/{id} 请求更新待处理订单，入参 order_id 和 status，返回 status 和 error_code，订单满足条件，返回更新成功，每次更新写审计日志，需要自动化测试覆盖成功与失败";

fn init(dir: &std::path::Path) {
    std::fs::write(dir.join("README.md"), "# demo\n").unwrap();
    run(&CommandRequest {
        command: "init".into(),
        cwd: dir.to_string_lossy().to_string(),
        args: None,
    })
    .unwrap();
}

fn command(
    dir: &std::path::Path,
    name: &str,
    args: serde_json::Value,
) -> sdd_core::contracts::CommandResult {
    run(&CommandRequest {
        command: name.into(),
        cwd: dir.to_string_lossy().to_string(),
        args: Some(args),
    })
    .unwrap()
}

#[test]
fn change_updates_existing_requirement() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let created = command(
        dir.path(),
        "new",
        json!({ "requirement": INITIAL_REQUIREMENT }),
    );
    assert_eq!(created.state, "SPEC_READY");
    let change_id = sdd_core::state::StateStore::new(dir.path().to_string_lossy().to_string())
        .read()
        .unwrap()
        .current_change_id
        .unwrap();

    let revised = command(
        dir.path(),
        "change",
        json!({ "changeId": change_id, "requirement": REVISED_REQUIREMENT }),
    );
    assert!(revised.ok);
    assert_eq!(revised.state, "SPEC_READY");
}

fn create_change(dir: &std::path::Path) -> String {
    let created = command(dir, "new", json!({ "requirement": INITIAL_REQUIREMENT }));
    assert_eq!(created.state, "SPEC_READY");
    sdd_core::state::StateStore::new(dir.to_string_lossy().to_string())
        .read()
        .unwrap()
        .current_change_id
        .unwrap()
}

fn cwd(dir: &std::path::Path) -> String {
    dir.to_string_lossy().to_string()
}

#[test]
fn change_rewrites_current_documents_without_revision_history() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let change_id = create_change(dir.path());
    let change_dir = dir.path().join(".sdd/changes").join(&change_id);
    for (name, content) in [
        ("design.md", "旧设计"),
        ("plan.md", "旧计划"),
        ("tasks.md", "旧任务"),
    ] {
        std::fs::write(change_dir.join(name), content).unwrap();
    }

    let revised = command(
        dir.path(),
        "change",
        json!({ "changeId": change_id, "requirement": REVISED_REQUIREMENT }),
    );
    let data = revised.data.unwrap();
    assert!(change_dir.join("proposal.md").exists());
    assert!(change_dir.join("spec.md").exists());
    assert!(!change_dir.join("design.md").exists());
    assert!(!change_dir.join("plan.md").exists());
    assert!(!change_dir.join("tasks.md").exists());
    assert!(!change_dir.join("revisions").exists());
    assert!(data.get("revisionId").is_none());
    assert!(data.get("diffPath").is_none());
    assert!(data.get("snapshotPath").is_none());

    for document in ["spec.md", "proposal.md"] {
        let content = std::fs::read_to_string(change_dir.join(document)).unwrap();
        assert!(content.contains(REVISED_REQUIREMENT));
    }
    let runtime = sdd_core::state::runtime_store::RuntimeStore::new(cwd(dir.path()))
        .read()
        .unwrap();
    let change = runtime.changes.get(&change_id).unwrap();
    assert!(change.get("revision").is_none());
    assert!(change.get("design").is_none());
    assert!(change.get("plan").is_none());
    assert_eq!(runtime.state.current_phase, "SPEC_READY");
    assert_eq!(
        runtime
            .runs
            .get(runtime.state.current_run_id.as_ref().unwrap())
            .and_then(|run| run.get("events"))
            .and_then(|events| events.as_array())
            .unwrap()
            .iter()
            .filter(|event| {
                event.get("type").and_then(|value| value.as_str()) == Some("REQUIREMENT_REVISED")
            })
            .count(),
        1
    );
    let spec = std::fs::read_to_string(change_dir.join("spec.md")).unwrap();
    let parsed = sdd_core::engines::openspec::parser::parse_spec(&spec).unwrap();
    assert_eq!(parsed.requirements.len(), 3);
}

#[test]
fn change_event_does_not_contain_revision_id() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let change_id = create_change(dir.path());
    command(
        dir.path(),
        "change",
        json!({ "changeId": change_id, "requirement": REVISED_REQUIREMENT }),
    );
    let runtime = sdd_core::state::runtime_store::RuntimeStore::new(cwd(dir.path()))
        .read()
        .unwrap();
    let events = runtime
        .runs
        .get(runtime.state.current_run_id.as_ref().unwrap())
        .and_then(|run| run.get("events"))
        .and_then(|events| events.as_array())
        .unwrap();
    let revised = events
        .iter()
        .find(|event| {
            event.get("type").and_then(|value| value.as_str()) == Some("REQUIREMENT_REVISED")
        })
        .unwrap();
    assert!(revised.get("revisionId").is_none());
}

#[test]
fn change_rejects_empty_requirement_with_stable_error() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let change_id = create_change(dir.path());
    let error = run(&CommandRequest {
        command: "change".into(),
        cwd: cwd(dir.path()),
        args: Some(json!({ "changeId": change_id, "requirement": "  " })),
    })
    .unwrap_err();
    assert_eq!(error.code, "E_INVALID_REQUIREMENT");
}

#[test]
fn change_rejects_oversized_requirement_before_state_access() {
    let dir = tempfile::tempdir().unwrap();
    let args = json!({
        "changeId": "oversized-requirement",
        "requirement": "需".repeat(32_769),
    });
    let error = sdd_core::commands::change::run_change(
        dir.path().to_string_lossy().as_ref(),
        Some(&args),
        &sdd_core::engines::spec::spec_engine::SpecEngine::new(),
    )
    .unwrap_err();

    assert_eq!(error.code, "E_INVALID_REQUIREMENT");
}

#[cfg(unix)]
#[test]
fn change_rejects_symlinked_managed_document() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());
    let change_id = create_change(dir.path());
    let spec_path = dir
        .path()
        .join(".sdd/changes")
        .join(&change_id)
        .join("spec.md");
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), "不得读取或改写").unwrap();
    std::fs::remove_file(&spec_path).unwrap();
    std::os::unix::fs::symlink(outside.path(), &spec_path).unwrap();

    let error = run(&CommandRequest {
        command: "change".into(),
        cwd: cwd(dir.path()),
        args: Some(json!({
            "changeId": change_id,
            "requirement": REVISED_REQUIREMENT
        })),
    })
    .unwrap_err();

    assert_eq!(error.code, "E_SYMLINK_BLOCKED");
    assert_eq!(
        std::fs::read_to_string(outside.path()).unwrap(),
        "不得读取或改写"
    );
}
