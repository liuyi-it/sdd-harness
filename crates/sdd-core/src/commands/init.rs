//! init 命令：创建 `.sdd/` 基础目录、写入默认配置并初始化状态。
//!
//! 所有机器数据写入 `.sdd/runtime.json`；项目接入文件写入所选 Agent 目录。
//! 知识图谱索引由 knowledge 模块接入，不托管外部服务进程。

use std::path::PathBuf;

use serde_json::json;

use crate::contracts::{CommandResult, HostAdapter};
use crate::error::SddError;
use crate::state::file_lock::lock_sdd;
use crate::state::runtime_store::RUNTIME_FILE;
use crate::state::state_store::{INDEX_STATUS_INDEX_READY, INDEX_STATUS_UNAVAILABLE};
use crate::state::StateStore;

/// 配置 schema 版本（Rust 版新格式）
pub const CONFIG_SCHEMA_VERSION: u32 = 3;

pub fn run_init(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    let timeout_ms = args
        .and_then(|a| a.get("timeout"))
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0) as u64);
    let _guard = lock_sdd(cwd, "sdd init", None, timeout_ms)?;

    let adapter = requested_adapter(args)?;

    let store = StateStore::new(cwd.to_string());
    let previous = store.read()?;
    let first_init = !previous.initialized;

    if first_init {
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

    // 写入默认配置到 runtime.json；重复 init 仅更新显式指定的结构策略。
    write_default_config(cwd, structure_policy, adapter)?;

    // 写资产前检测会被覆盖的既有适配器文件（目标已存在且内容与嵌入模板不同），
    // 复跑 init 时提示用户本地文件被模板覆盖。
    let overwritten = crate::assets::detect_overwrites(cwd, adapter);
    let adapter_files = crate::assets::write_adapter_files(cwd, adapter)?;

    // 空项目检测：无源文件时附加 warning（一期不做 CLARIFYING 暂停）
    let empty_project = is_empty_project(cwd);
    let mut warnings: Vec<serde_json::Value> = Vec::new();
    for target in adapter_files {
        warnings.push(json!({
            "code": "W_ADAPTER_FILE",
            "message": format!("写入：{target}"),
        }));
    }
    for target in overwritten {
        warnings.push(json!({
            "code": "W_ADAPTER_OVERWRITE",
            "message": format!("已覆盖与嵌入模板不一致的适配器文件：{target}"),
        }));
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
        s.degraded_reason = degraded.then(|| "CodeGraph 未成功索引，使用受限文件扫描".to_string());
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

/// 写默认配置到 runtime.json 的 config 节点。
fn write_default_config(
    cwd: &str,
    structure_policy: Option<&str>,
    adapter: HostAdapter,
) -> Result<(), SddError> {
    let project_name = cwd
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("auto-detect");
    let mut config = crate::state::runtime_store::read_config(cwd)?;
    if config.is_null() || config == json!({}) {
        config = json!({
            "schemaVersion": CONFIG_SCHEMA_VERSION,
            "project": { "name": project_name },
            "hostAdapter": adapter.as_str(),
            "codebase": {
                "providers": ["codegraph"],
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
                "requireDriftCheck": true,
                "ocr": { "mode": "auto", "command": "ocr" }
            },
            "contextPack": { "maxSizeKb": 30 },
            "audit": { "maxSizeMb": 5, "maxFiles": 200 },
            "git": { "createBranch": false, "createWorktree": false },
            "security": {
                "blockOutsideRepo": true,
                "blockSymlinksOutsideRepo": true,
                "redactSecretsInLogs": true
            }
        });
    }
    if !config.is_object()
        || !config
            .get("workflow")
            .is_some_and(serde_json::Value::is_object)
    {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "runtime.json 的 config 必须包含 workflow 对象",
        ));
    }
    {
        let config_object = config.as_object_mut().expect("已验证 config 为对象");
        config_object.insert("schemaVersion".to_string(), json!(CONFIG_SCHEMA_VERSION));
        config_object.insert("hostAdapter".to_string(), json!(adapter.as_str()));
        config_object.remove("plugins");
        if let Some(policy) = structure_policy {
            config_object
                .get_mut("workflow")
                .and_then(serde_json::Value::as_object_mut)
                .ok_or_else(|| {
                    SddError::new(
                        "E_STATE_CORRUPTED",
                        "runtime.json 的 config 缺少 workflow 对象",
                    )
                })?
                .insert("structurePolicy".to_string(), json!(policy));
        }
    }
    crate::state::runtime_store::write_config(cwd, config)
}

/// 解析并校验宿主注入的 Agent 适配器。
fn requested_adapter(args: Option<&serde_json::Value>) -> Result<HostAdapter, SddError> {
    if args.and_then(|value| value.get("agent")).is_some() {
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            "不支持 agent 参数；请使用 hostAdapter",
        ));
    }
    let raw = match args.and_then(|value| value.get("hostAdapter")) {
        None => return Ok(HostAdapter::DEFAULT),
        Some(value) => value.as_str().ok_or_else(|| {
            SddError::new(
                "E_INVALID_PHASE_COMMAND",
                "hostAdapter 必须是字符串；仅支持 codex 或 omp",
            )
        })?,
    };
    HostAdapter::parse(raw)
        .ok_or_else(|| SddError::new("E_INVALID_PHASE_COMMAND", "Agent 仅支持 Codex 或 OMP"))
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
    let root = PathBuf::from(cwd);
    for marker in markers {
        if root.join(marker).exists() {
            return false;
        }
    }
    true
}

/// 供测试与后续任务复用的状态文件路径
pub fn state_path(cwd: &str) -> PathBuf {
    PathBuf::from(cwd).join(".sdd").join(RUNTIME_FILE)
}
