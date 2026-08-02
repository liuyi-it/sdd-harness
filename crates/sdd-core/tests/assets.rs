//! 资产嵌入与 adapter 写入测试。

use sdd_core::assets::{assets_for_agent, write_adapter_files, ADAPTER_ASSETS};

#[test]
fn assets_are_embedded() {
    assert!(!ADAPTER_ASSETS.is_empty());
    // claude 有命令模板
    assert!(ADAPTER_ASSETS
        .iter()
        .any(|a| a.key == "claude-code/commands/sdd.auto.md"));
    // codex 有规则
    assert!(ADAPTER_ASSETS
        .iter()
        .any(|a| a.key == "codex/rules/sdd-harness.md"));
}

#[test]
fn assets_for_agent_filters_by_prefix() {
    let claude = assets_for_agent("claude");
    assert!(claude.iter().any(|k| k.starts_with("claude-code/")));
    assert!(!claude.iter().any(|k| k.starts_with("codex/")));
    let codex = assets_for_agent("codex");
    assert!(codex.iter().any(|k| k.starts_with("codex/")));
}

#[test]
fn init_writes_adapter_files_for_claude() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let written = write_adapter_files(&cwd, "claude", false).unwrap();
    assert!(!written.is_empty());
    assert!(dir.path().join(".claude/commands/sdd.auto.md").exists());
    assert!(dir.path().join(".claude/commands/sdd.status.md").exists());
}

#[test]
fn init_with_codex_writes_rules() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    write_adapter_files(&cwd, "codex", false).unwrap();
    assert!(dir.path().join(".codex/rules/sdd-harness.md").exists());
    assert!(dir.path().join(".codex/skills/sdd-harness/sdd.md").exists());
}

#[test]
fn write_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    write_adapter_files(&cwd, "claude", false).unwrap();
    // 第二次：内容相同 → 全部跳过（无新写入）
    let second = write_adapter_files(&cwd, "claude", false).unwrap();
    assert!(second
        .iter()
        .all(|s| s.contains("跳过") || s.contains("写入")));
}

#[test]
fn init_command_integrates_adapter_write() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# demo").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let result = sdd_core::run(&sdd_core::contracts::CommandRequest {
        command: "init".into(),
        cwd: cwd.clone(),
        args: Some(serde_json::json!({ "agent": "codex" })),
    })
    .unwrap();
    assert!(result.ok);
    assert!(dir.path().join(".codex/rules/sdd-harness.md").exists());
}

#[test]
fn refresh_keeps_user_agents_content() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "# 用户规则\n\n保留我\n").unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    write_adapter_files(&cwd, "claude", false).unwrap();
    write_adapter_files(&cwd, "claude", false).unwrap();
    let content = std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert!(content.contains("# 用户规则"));
    assert!(content.contains("保留我"));
    assert_eq!(content.matches("<!-- sdd-harness:managed -->").count(), 1);
    assert_eq!(
        content.matches("<!-- sdd-harness:managed:end -->").count(),
        1
    );
}
