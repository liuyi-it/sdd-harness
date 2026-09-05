//! init 命令：创建 `.sdd/` 基础目录、写入默认配置并初始化状态。
//!
//! 所有机器数据写入 `.sdd/runtime.json`；项目接入文件写入所选 Agent 目录。
//! 知识图谱索引由 knowledge 模块接入，不托管外部服务进程。

use std::path::PathBuf;

use serde_json::json;

use crate::contracts::{CliWarning, CommandResult, HostAdapter};
use crate::error::SddError;
use crate::state::file_lock::lock_sdd;

pub fn run_init(cwd: &str, args: Option<&serde_json::Value>) -> Result<CommandResult, SddError> {
    super::validate_args(args, &["timeout", "structurePolicy", "hostAdapter"])?;
    let timeout_ms = super::timeout_ms(args)?;
    let _guard = lock_sdd(cwd, timeout_ms)?;
    let adapter = requested_adapter(args)?;
    let structure_policy = super::string_arg(args, "structurePolicy")?;
    if structure_policy.is_some_and(|policy| !matches!(policy, "free-design" | "user-defined")) {
        return Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            "structurePolicy 仅支持 free-design 或 user-defined",
        ));
    }

    // 首次初始化状态和当前配置在同一事务提交；重复 init 只更新当前配置。
    let (first_init, _) =
        crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
            let first_init = !document.state.initialized;
            apply_config(document, structure_policy, adapter)?;
            if first_init {
                crate::state::state_store::apply_state_update(&mut document.state, |state| {
                    state.current_phase = "INITIALIZING".to_string();
                    state.last_command = Some("sdd init".to_string());
                })?;
            }
            Ok(first_init)
        })?;

    let adapter_files = crate::assets::write_adapter_files(cwd, adapter)?;

    // 空项目检测：无源文件时附加 warning，不改变初始化状态机。
    let empty_project = is_empty_project(cwd);
    let mut warnings = Vec::new();
    for target in adapter_files.written {
        warnings.push(CliWarning::new("W_ADAPTER_FILE", format!("写入：{target}")));
    }
    for target in adapter_files.overwritten {
        warnings.push(CliWarning::new(
            "W_ADAPTER_OVERWRITE",
            format!("已覆盖与嵌入模板不一致的适配器文件：{target}"),
        ));
    }
    if first_init && empty_project && structure_policy.is_none() {
        warnings.push(CliWarning::new(
            "W_EMPTY_PROJECT",
            "空项目已就绪，可直接描述需求；目录结构将在规格阶段确定",
        ));
    }

    let index_timeout_ms = timeout_ms.unwrap_or(60_000);
    let index = crate::knowledge::router::KnowledgeRouter::new().initialize(cwd, index_timeout_ms);
    let diagnostic = index
        .diagnostics
        .first()
        .expect("CodeGraph 必须返回一条索引诊断");
    if diagnostic.degraded {
        let reason = diagnostic
            .reason
            .as_deref()
            .expect("失败的 CodeGraph 索引诊断必须提供原因");
        warnings.push(
            CliWarning::new(
                "W_KNOWLEDGE_UNAVAILABLE",
                format!(
                    "知识图谱引擎不可用（{}：{}）：已降级为受限文件扫描",
                    diagnostic.provider, reason
                ),
            )
            .with_next("sdd codebase doctor"),
        );
    }
    let (_, document) =
        crate::state::RuntimeStore::new(cwd.to_string()).try_update(|document| {
            crate::commands::codebase::apply_index(cwd, document, &index, |state| {
                state.initialized = true;
                if first_init {
                    state.current_phase = "INDEX_READY".to_string();
                }
                state.last_command = Some("sdd init".to_string());
            })
        })?;
    let ready = document.state;

    Ok(CommandResult {
        ok: true,
        state: ready.current_phase.clone(),
        exit_code: 0,
        change_id: None,
        next: crate::commands::status::next_command(&ready.current_phase),
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

/// 将当前配置写入待提交的 runtime 文档。
fn apply_config(
    document: &mut crate::state::RuntimeDocument,
    structure_policy: Option<&str>,
    adapter: HostAdapter,
) -> Result<(), SddError> {
    let config = &mut document.config;
    {
        let config_object = config
            .as_object_mut()
            .expect("当前 config schema 保证为对象");
        config_object.insert("hostAdapter".to_string(), json!(adapter.as_str()));
        if let Some(policy) = structure_policy {
            config_object
                .get_mut("workflow")
                .and_then(serde_json::Value::as_object_mut)
                .expect("当前 config schema 保证 workflow 为对象")
                .insert("structurePolicy".to_string(), json!(policy));
        }
    }
    crate::schema::validate_json("config", config)
}

/// 解析并校验宿主注入的 Agent 适配器。
fn requested_adapter(args: Option<&serde_json::Value>) -> Result<HostAdapter, SddError> {
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
