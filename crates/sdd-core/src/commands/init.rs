//! init 命令：创建 `.sdd/` 基础目录、写入默认配置并初始化状态。
//!
//! 翻译自 Node 版 `packages/core/src/commands/init.ts`（不含 codebase-memory-mcp
//! 托管与依赖完整性校验；知识图谱索引由 knowledge 模块接入）。
//! 配置格式由 YAML 重构为 JSON（config.json，允许重构决策）。

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::state::file_lock::lock_sdd;
use crate::state::state_store::{WorkflowState, INDEX_STATUS_INDEX_READY, STATE_FILE};
use crate::state::StateStore;

/// 配置 schema 版本（Rust 版新格式）
pub const CONFIG_SCHEMA_VERSION: u32 = 2;

pub fn run_init(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    let timeout_ms = args
        .and_then(|a| a.get("timeout"))
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0) as u64);
    let _guard = lock_sdd(cwd, "sdd init", None, timeout_ms)?;

    let sdd_root = PathBuf::from(cwd).join(".sdd");
    fs::create_dir_all(&sdd_root)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("创建 .sdd 目录失败：{e}")))?;

    let store = StateStore::new(cwd.to_string());
    let state_path = store.state_path();

    // 已初始化时幂等返回（不重置现有状态）
    if state_path.exists() {
        let state = store.read()?;
        return Ok(CommandResult {
            ok: true,
            state: state.current_phase.clone(),
            exit_code: 0,
            change_id: state.current_change_id.clone(),
            next: crate::commands::status::next_command(&state.current_phase),
            data: None,
            rendered: None,
            warnings: None,
            action_required: None,
            error: None,
        });
    }

    // 写初始状态并推进到 INITIALIZING
    store.write(&WorkflowState::not_initialized())?;
    let _initializing = store.update(|s| {
        s.current_phase = "INITIALIZING".to_string();
        s.in_progress_phase = Some("INITIALIZING".to_string());
        s.last_command = Some("sdd init".to_string());
        s.last_error = None;
    })?;

    // 写入默认配置（config.json）
    write_default_config(cwd, &sdd_root)?;

    // 写入 Agent 接入文件（--agent 指定，默认 claude；幂等）
    let agent = args
        .and_then(|a| a.get("agent"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("claude");
    let force = args
        .and_then(|a| a.get("force"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let adapter_files =
        crate::assets::write_adapter_files(cwd, agent, force).unwrap_or_else(|_| Vec::new());

    // 空项目检测：无源文件时附加 warning（一期不做 CLARIFYING 暂停）
    let empty_project = is_empty_project(cwd);
    let mut warnings: Vec<serde_json::Value> = Vec::new();
    for file in adapter_files {
        warnings.push(json!({ "code": "W_ADAPTER_FILE", "message": file }));
    }
    if empty_project {
        warnings.push(json!({
            "code": "W_EMPTY_PROJECT",
            "message": "空项目需要先通过 structurePolicy 指定目录结构策略，可选 free-design 或 user-defined",
        }));
    }

    // 知识图谱索引（knowledge 模块接入后填充诊断；当前先写空诊断）
    let knowledge_diags = crate::knowledge::router::KnowledgeRouter::new().initialize(cwd);
    for diag in &knowledge_diags {
        if diag
            .get("degraded")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            warnings.push(json!({
                "code": "W_KNOWLEDGE_UNAVAILABLE",
                "message": format!(
                    "知识图谱引擎不可用（{}）：已降级为受限文件扫描",
                    diag.get("provider").and_then(|v| v.as_str()).unwrap_or("?")
                ),
                "next": "sdd codebase doctor",
            }));
        }
    }

    // 收敛到 INDEX_READY
    let ready = store.update(|s| {
        s.initialized = true;
        s.current_phase = "INDEX_READY".to_string();
        s.previous_phase = Some("NOT_INITIALIZED".to_string());
        s.in_progress_phase = None;
        s.index_status = INDEX_STATUS_INDEX_READY.to_string();
        // codebaseProvider 由 knowledge 初始化结果填充
        if let Some(first) = knowledge_diags.first() {
            if first
                .get("installed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                if let Some(provider) = first.get("provider").and_then(|v| v.as_str()) {
                    s.codebase_provider = provider.to_string();
                }
            }
        }
        s.suggested_command = Some("sdd new".to_string());
        s.last_command = Some("sdd init".to_string());
        s.last_error = None;
    })?;

    let _ = ready;
    Ok(CommandResult {
        ok: true,
        state: "INDEX_READY".to_string(),
        exit_code: 0,
        change_id: None,
        next: Some("sdd new".to_string()),
        data: None,
        rendered: None,
        warnings: if warnings.is_empty() {
            None
        } else {
            Some(warnings)
        },
        action_required: None,
        error: None,
    })
}

/// 写默认配置 config.json（对应 Node 版 defaultConfig，格式从 YAML 改为 JSON）
fn write_default_config(cwd: &str, sdd_root: &std::path::Path) -> Result<(), SddError> {
    let project_name = cwd
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("auto-detect");
    let config = json!({
        "schemaVersion": CONFIG_SCHEMA_VERSION,
        "project": { "name": project_name },
        "plugins": {
            "claudeCode": { "enabled": true },
            "codex": { "enabled": true },
            "openCode": { "enabled": true }
        },
        "codebase": {
            "providers": ["gitnexus", "codegraph"],
            "fallbackProvider": "file-scan",
            "autoIndexOnInit": true
        },
        "workflow": {
            "maxClarifyingQuestionsPerRound": 5,
            "requireBlockerAnswers": true,
            "stopOnFailure": true
        },
        "quality": {
            "requireFileScopeCheck": true,
            "requireDriftCheck": true
        },
        "contextPack": { "maxSizeKb": 30 },
        "audit": { "maxSizeMb": 10, "maxFiles": 5 },
        "git": { "createBranch": false, "createWorktree": false },
        "security": {
            "blockOutsideRepo": true,
            "blockSymlinksOutsideRepo": true,
            "redactSecretsInLogs": true
        }
    });
    let path = sdd_root.join("config.json");
    if !path.exists() {
        let content = serde_json::to_string_pretty(&config)
            .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化配置失败：{e}")))?;
        fs::write(&path, content).map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("写入 config.json 失败：{e}"))
        })?;
    }
    Ok(())
}

/// 空项目检测：无 README/源文件/包清单时视为空项目
fn is_empty_project(cwd: &str) -> bool {
    let markers = [
        "README.md",
        "package.json",
        "Cargo.toml",
        "pyproject.toml",
        "go.mod",
        "pom.xml",
        "src",
        "lib",
        "tests",
    ];
    for marker in markers {
        if PathBuf::from(cwd).join(marker).exists() {
            return false;
        }
    }
    true
}

/// 供测试与后续任务复用的状态文件路径
pub fn state_path(cwd: &str) -> PathBuf {
    PathBuf::from(cwd).join(".sdd").join(STATE_FILE)
}
