use sdd_core::contracts::{AgentActionRequired, CommandRequest, CommandResult};
use sdd_core::run;
use serde_json::{json, Value};

fn command(root: &std::path::Path, name: &str, args: Option<Value>) -> CommandResult {
    run(&CommandRequest {
        command: name.to_string(),
        cwd: root.to_string_lossy().into_owned(),
        args,
    })
    .unwrap_or_else(|error| panic!("{name} 失败：{} {}", error.code, error.message))
}

fn spec_result() -> Value {
    json!({
        "schemaVersion": "4.0.0",
        "goal": "让用户看到明确的完成结果",
        "scope": { "included": ["更新 README 行为"], "excluded": ["不修改依赖"] },
        "constraints": ["保持现有接口"],
        "model": { "requirements": [{
            "id": "REQ-001",
            "title": "完成行为",
            "statement": "系统必须提供可验证的完成行为",
            "scenarios": [{
                "id": "REQ-001-SC-001",
                "title": "成功完成",
                "given": ["项目已初始化"],
                "when": ["用户执行功能"],
                "then": ["用户看到完成结果"]
            }]
        }]}
    })
}

fn design_result() -> Value {
    json!({
        "schemaVersion": "1.0.0",
        "summary": "在既有 README 入口完成最小实现",
        "currentState": ["README 已存在"],
        "decisions": [{ "title": "复用入口", "decision": "修改 README", "rationale": "范围最小且可验证" }],
        "affectedFiles": ["README.md"],
        "interfaces": ["README 文档接口"],
        "dataChanges": [],
        "errorHandling": ["内部不变量失败时直接失败"],
        "testStrategy": ["运行 cargo test"],
        "risks": ["文档与行为不一致"],
        "rollback": ["使用 Git 回退提交"]
    })
}

fn plan_result() -> Value {
    json!({
        "schemaVersion": "3.0.0",
        "summary": "一个纵向任务完成行为和验证",
        "globalConstraints": ["只修改 README.md"],
        "dependencies": [],
        "tasks": [{
            "id": "TASK-001",
            "title": "完成并验证用户行为",
            "executionMode": "TDD",
            "requirements": ["REQ-001"],
            "scenarios": ["REQ-001-SC-001"],
            "dependsOn": [],
            "allowedFiles": ["README.md"],
            "expectedNewFiles": [],
            "forbiddenFiles": ["Cargo.toml"],
            "interfaces": { "consumes": ["现有 README"], "produces": ["更新后的 README"] },
            "steps": [
                { "kind": "TEST", "instruction": "先验证现有内容不满足需求" },
                { "kind": "IMPLEMENT", "instruction": "更新用户可见行为" },
                { "kind": "VERIFY", "instruction": "执行完整测试" }
            ],
            "verification": [{ "command": "cargo", "args": ["test"], "expected": "全部测试通过" }],
            "doneCriteria": ["行为和测试一致"],
            "userVisibleOutcome": "用户看到完成结果",
            "acceptanceCriteria": ["成功场景通过"],
            "testSeam": "README.md"
        }]
    })
}

#[test]
fn full_staged_workflow_reaches_archive() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# fixture\n").unwrap();
    assert_eq!(command(dir.path(), "init", None).state, "INDEX_READY");

    let started = command(
        dir.path(),
        "new",
        Some(json!({ "changeId": "demo", "requirement": "实现可验证的完成行为" })),
    );
    assert_eq!(started.state, "SPEC_WAITING_AGENT");
    assert!(matches!(
        started.action_required,
        Some(AgentActionRequired::AgentPhaseExecution { .. })
    ));
    assert_eq!(
        command(
            dir.path(),
            "new",
            Some(json!({ "changeId": "demo", "resultJson": spec_result().to_string() })),
        )
        .state,
        "SPEC_READY"
    );
    assert_eq!(
        command(dir.path(), "design", Some(json!({ "changeId": "demo" }))).state,
        "DESIGN_WAITING_AGENT"
    );
    assert_eq!(
        command(
            dir.path(),
            "design",
            Some(json!({ "changeId": "demo", "resultJson": design_result().to_string() })),
        )
        .state,
        "DESIGN_READY"
    );
    assert_eq!(
        command(dir.path(), "plan", Some(json!({ "changeId": "demo" }))).state,
        "PLAN_WAITING_AGENT"
    );
    assert_eq!(
        command(
            dir.path(),
            "plan",
            Some(json!({ "changeId": "demo", "resultJson": plan_result().to_string() })),
        )
        .state,
        "PLAN_READY"
    );

    let build = command(
        dir.path(),
        "build",
        Some(json!({ "changeId": "demo", "sub": "next" })),
    );
    assert!(matches!(
        build.action_required,
        Some(AgentActionRequired::AgentTaskExecution { .. })
    ));
    let result = json!({
        "taskId": "TASK-001",
        "status": "completed",
        "filesChanged": [],
        "evidence": [
            { "type": "command-run", "command": "cargo test", "passed": false, "expectedFailure": true, "output": "预期失败" },
            { "type": "command-run", "command": "cargo test", "passed": true, "output": "通过" }
        ],
        "verification": [{ "command": "cargo", "args": ["test"], "passed": true, "output": "通过" }]
    });
    assert_eq!(
        command(
            dir.path(),
            "build",
            Some(json!({
                "changeId": "demo", "sub": "complete", "task": "TASK-001",
                "resultJson": result.to_string()
            })),
        )
        .state,
        "BUILD_READY"
    );
    assert_eq!(
        command(dir.path(), "verify", Some(json!({ "changeId": "demo" }))).state,
        "QUALITY_READY"
    );
    assert_eq!(
        command(dir.path(), "archive", Some(json!({ "changeId": "demo" }))).state,
        "ARCHIVED"
    );
    assert!(dir.path().join(".sdd/changes/demo/archive.md").is_file());
    assert!(!dir.path().join(".sdd/changes/demo/spec.md").exists());
}

#[test]
fn multiple_active_changes_require_explicit_selection() {
    let dir = tempfile::tempdir().unwrap();
    command(dir.path(), "init", None);
    for change_id in ["alpha", "beta"] {
        command(
            dir.path(),
            "new",
            Some(json!({ "changeId": change_id, "requirement": format!("实现 {change_id}") })),
        );
    }
    let status = command(dir.path(), "status", None);
    assert_eq!(status.state, "MULTIPLE_CHANGES");
    assert_eq!(
        status.data.unwrap()["activeChanges"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let error = run(&CommandRequest {
        command: "design".to_string(),
        cwd: dir.path().to_string_lossy().into_owned(),
        args: None,
    })
    .unwrap_err();
    assert_eq!(error.code, "E_CHANGE_SELECTION_REQUIRED");
}

#[test]
fn old_runtime_version_is_rejected_without_migration() {
    let dir = tempfile::tempdir().unwrap();
    let sdd = dir.path().join(".sdd");
    std::fs::create_dir(&sdd).unwrap();
    let raw = "{\"schemaVersion\":5}";
    std::fs::write(sdd.join("runtime.json"), raw).unwrap();
    std::fs::write(
        sdd.join("runtime.json.sha256"),
        format!("{}\n", sdd_core::state::checksum::compute(raw.as_bytes())),
    )
    .unwrap();
    let error = sdd_core::state::RuntimeStore::new(dir.path())
        .read()
        .unwrap_err();
    assert_eq!(error.code, "E_STATE_VERSION_UNSUPPORTED");
}

#[test]
fn quality_fix_runs_once_then_requires_user_authorization() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    std::fs::write(dir.path().join("README.md"), "# fixture\n").unwrap();
    git(dir.path(), &["add", "README.md"]);
    git(dir.path(), &["commit", "-m", "init"]);

    command(dir.path(), "init", None);
    command(
        dir.path(),
        "new",
        Some(json!({ "changeId": "quality", "requirement": "实现质量修复预算" })),
    );
    command(
        dir.path(),
        "new",
        Some(json!({ "changeId": "quality", "resultJson": spec_result().to_string() })),
    );
    command(dir.path(), "design", Some(json!({ "changeId": "quality" })));
    command(
        dir.path(),
        "design",
        Some(json!({ "changeId": "quality", "resultJson": design_result().to_string() })),
    );
    command(dir.path(), "plan", Some(json!({ "changeId": "quality" })));
    command(
        dir.path(),
        "plan",
        Some(json!({ "changeId": "quality", "resultJson": plan_result().to_string() })),
    );
    command(
        dir.path(),
        "build",
        Some(json!({ "changeId": "quality", "sub": "next" })),
    );
    std::fs::write(
        dir.path().join("README.md"),
        "# fixture\nAuthorization: Bearer secret-value\n",
    )
    .unwrap();
    let task_result = json!({
        "taskId": "TASK-001",
        "status": "completed",
        "filesChanged": ["README.md"],
        "evidence": [
            { "type": "command-run", "command": "cargo test", "passed": false, "expectedFailure": true, "output": "预期失败" },
            { "type": "command-run", "command": "cargo test", "passed": true, "output": "通过" }
        ],
        "verification": [{ "command": "cargo", "args": ["test"], "passed": true, "output": "通过" }]
    });
    command(
        dir.path(),
        "build",
        Some(json!({
            "changeId": "quality", "sub": "complete", "task": "TASK-001",
            "resultJson": task_result.to_string()
        })),
    );
    let first = command(dir.path(), "verify", Some(json!({ "changeId": "quality" })));
    assert_eq!(first.state, "QUALITY_WAITING_FIX");
    assert!(matches!(
        first.action_required,
        Some(AgentActionRequired::AgentFixExecution { .. })
    ));

    let fix_result = json!({
        "fixId": "FIX-001",
        "status": "completed",
        "filesChanged": ["README.md"],
        "verification": [{ "command": "cargo", "args": ["test"], "passed": true, "output": "通过" }]
    });
    let blocked = command(
        dir.path(),
        "verify",
        Some(json!({ "changeId": "quality", "resultJson": fix_result.to_string() })),
    );
    assert_eq!(blocked.state, "QUALITY_BLOCKED");
    assert!(!blocked.ok);

    let continued = command(
        dir.path(),
        "verify",
        Some(json!({ "changeId": "quality", "continue": true })),
    );
    assert_eq!(continued.state, "QUALITY_WAITING_FIX");
    match continued.action_required {
        Some(AgentActionRequired::AgentFixExecution { fix_id, .. }) => {
            assert_eq!(fix_id, "FIX-002")
        }
        other => panic!("期望第二轮修复行动，实际：{other:?}"),
    }
}

fn git(root: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} 失败：{}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}
