//! init 命令：创建 `.sdd/` 基础目录、写入默认配置并初始化状态。
//!
//! 知识图谱索引由 knowledge 模块接入，不托管外部服务进程。
//! 配置格式由 YAML 重构为 JSON（config.json，允许重构决策）。

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::contracts::CommandResult;
use crate::error::SddError;
use crate::state::file_lock::lock_sdd;
use crate::state::state_store::{
    WorkflowState, INDEX_STATUS_INDEX_READY, INDEX_STATUS_UNAVAILABLE, STATE_FILE,
};
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
    let previous = store.read()?;
    let first_init = !previous.initialized;

    if first_init {
        store.write(&WorkflowState::not_initialized())?;
        store.update(|s| {
            s.current_phase = "INITIALIZING".to_string();
            s.in_progress_phase = Some("INITIALIZING".to_string());
            s.last_command = Some("sdd init".to_string());
            s.last_error = None;
        })?;
    }

    let structure_policy = args
        .and_then(|value| value.get("structurePolicy"))
        .and_then(|value| value.as_str());
    if structure_policy.is_some_and(|policy| !matches!(policy, "free-design" | "user-defined")) {
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            "structurePolicy 仅支持 free-design 或 user-defined",
        ));
    }

    // 写入默认配置（config.json）；重复 init 仅更新显式指定的结构策略。
    write_default_config(cwd, &sdd_root, structure_policy)?;

    // 写入 Agent 接入文件（支持逗号分隔；未指定时安装全部内置 Adapter）
    let agents = args
        .and_then(|a| a.get("agent"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("claude,codex,opencode");
    let force = args
        .and_then(|a| a.get("force"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut adapter_files = Vec::new();
    for agent in agents
        .split(',')
        .map(str::trim)
        .filter(|agent| !agent.is_empty())
    {
        if !crate::assets::known_agents().contains(&agent) {
            return Err(SddError::new(
                "E_INVALID_PHASE_COMMAND",
                &format!("未知 Agent：{agent}"),
            ));
        }
        adapter_files.extend(crate::assets::write_adapter_files(cwd, agent, force)?);
    }

    // 空项目检测：无源文件时附加 warning（一期不做 CLARIFYING 暂停）
    let empty_project = is_empty_project(cwd);
    let mut warnings: Vec<serde_json::Value> = Vec::new();
    for file in adapter_files {
        warnings.push(json!({ "code": "W_ADAPTER_FILE", "message": file }));
    }
    if first_init && empty_project && structure_policy.is_none() {
        warnings.push(json!({
            "code": "W_EMPTY_PROJECT",
            "message": "空项目需要先通过 structurePolicy 指定目录结构策略，可选 free-design 或 user-defined",
        }));
    }

    let index_timeout_ms = timeout_ms.unwrap_or(60_000);
    let knowledge_diags =
        crate::knowledge::router::KnowledgeRouter::new().initialize(cwd, index_timeout_ms)?;
    crate::commands::codebase::record_index_artifacts(cwd)?;
    for diag in &knowledge_diags {
        if !diag
            .get("indexed")
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

    let indexed_provider = knowledge_diags.iter().find(|diag| {
        diag.get("indexed")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    });
    let degraded = indexed_provider.is_none();

    let ready = store.update(|s| {
        s.initialized = true;
        if first_init {
            s.current_phase = "INDEX_READY".to_string();
            s.previous_phase = Some("NOT_INITIALIZED".to_string());
            s.in_progress_phase = None;
            s.suggested_command = Some("sdd new".to_string());
        }
        s.index_status = if degraded {
            INDEX_STATUS_UNAVAILABLE.to_string()
        } else {
            INDEX_STATUS_INDEX_READY.to_string()
        };
        s.codebase_provider = indexed_provider
            .and_then(|diag| diag.get("provider"))
            .and_then(|value| value.as_str())
            .unwrap_or("fallback-file-scan")
            .to_string();
        s.degraded = degraded;
        s.degraded_reason =
            degraded.then(|| "GitNexus 与 CodeGraph 均未成功索引，使用受限文件扫描".to_string());
        s.last_command = Some("sdd init".to_string());
        s.last_error = None;
    })?;

    Ok(CommandResult {
        ok: true,
        state: ready.current_phase.clone(),
        exit_code: 0,
        change_id: ready.current_change_id.clone(),
        next: crate::commands::status::next_command(&ready.current_phase)
            .or(ready.suggested_command.clone()),
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
fn write_default_config(
    cwd: &str,
    sdd_root: &std::path::Path,
    structure_policy: Option<&str>,
) -> Result<(), SddError> {
    let project_name = cwd
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("auto-detect");
    let mut config = json!({
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
            "stopOnFailure": true,
            "gitIsolation": false
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
    if path.exists() {
        let raw = fs::read_to_string(&path).map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("读取 config.json 失败：{e}"))
        })?;
        config = serde_json::from_str(&raw).map_err(|e| {
            SddError::new("E_STATE_CORRUPTED", &format!("config.json 解析失败：{e}"))
        })?;
    }
    if !config.is_object()
        || !config
            .get("workflow")
            .is_some_and(serde_json::Value::is_object)
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "config.json 必须包含 workflow 对象",
        ));
    }
    if let Some(policy) = structure_policy {
        config
            .get_mut("workflow")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "config.json 缺少 workflow 对象"))?
            .insert("structurePolicy".to_string(), json!(policy));
    }
    let content = serde_json::to_string_pretty(&config)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("序列化配置失败：{e}")))?;
    fs::write(&path, content)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("写入 config.json 失败：{e}")))?;
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
