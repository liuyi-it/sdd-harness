//! StateStore 负责工作流状态的读取、校验与更新。
//!
//! 状态是 `.sdd/runtime.json` 的 `state` 节点；runtime 文件由 `RuntimeStore`
//! 统一原子写入，避免状态、配置和制品索引之间出现半更新。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::SddError;
use crate::state::runtime_store::{RuntimeStore, RUNTIME_FILE};

pub const SDD_DIR: &str = ".sdd";
/// 保留常量名供 CLI 路径辅助函数使用；实际文件已统一为 runtime.json。
pub const STATE_FILE: &str = RUNTIME_FILE;

/// 当前状态节点 schema 版本。
pub const CURRENT_SCHEMA_VERSION: u32 = 3;

pub const TASK_STATUS_PENDING: &str = "PENDING";
pub const TASK_STATUS_BUILDING: &str = "BUILDING";
pub const TASK_STATUS_DONE: &str = "DONE";
pub const TASK_STATUS_FAILED: &str = "FAILED";
pub const TASK_STATUS_SKIPPED: &str = "SKIPPED";

pub const INDEX_STATUS_MISSING: &str = "MISSING";
pub const INDEX_STATUS_INDEXING: &str = "INDEXING";
pub const INDEX_STATUS_INDEX_READY: &str = "INDEX_READY";
pub const INDEX_STATUS_STALE: &str = "STALE";
pub const INDEX_STATUS_UNAVAILABLE: &str = "UNAVAILABLE";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    #[serde(default)]
    pub branch_name: Option<String>,
    #[serde(default)]
    pub worktree_path: Option<String>,
    pub baseline_commit: String,
    #[serde(default)]
    pub baseline_changed_files: Vec<String>,
    #[serde(default)]
    pub baseline_file_hashes: std::collections::BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub baseline_cargo_manifest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowState {
    pub schema_version: u32,
    pub version: u32,
    pub updated_at: String,
    pub initialized: bool,
    pub current_change_id: Option<String>,
    pub current_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_loop: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_agent_task: Option<serde_json::Value>,
    pub current_phase: String,
    pub index_status: String,
    pub codebase_provider: String,
    pub degraded: bool,
    pub degraded_reason: Option<String>,
    pub last_command: Option<String>,
    pub last_error: Option<String>,
    pub previous_phase: Option<String>,
    pub in_progress_phase: Option<String>,
    pub failed_command: Option<String>,
    pub failed_reason: Option<String>,
    pub interrupted_command: Option<String>,
    pub recoverable: bool,
    pub suggested_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceInfo>,
    #[serde(default)]
    pub tasks: HashMap<String, String>,
    #[serde(default)]
    pub artifacts: HashMap<String, String>,
}

impl Default for WorkflowState {
    fn default() -> Self {
        Self::not_initialized()
    }
}

impl WorkflowState {
    pub fn not_initialized() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            version: 1,
            updated_at: now_iso(),
            initialized: false,
            current_change_id: None,
            current_run_id: None,
            active_loop: None,
            pending_agent_task: None,
            current_phase: "NOT_INITIALIZED".to_string(),
            index_status: INDEX_STATUS_MISSING.to_string(),
            codebase_provider: "gitnexus".to_string(),
            degraded: false,
            degraded_reason: None,
            last_command: None,
            last_error: None,
            previous_phase: None,
            in_progress_phase: None,
            failed_command: None,
            failed_reason: None,
            interrupted_command: None,
            recoverable: true,
            suggested_command: Some("sdd init".to_string()),
            workspace: None,
            tasks: HashMap::new(),
            artifacts: HashMap::new(),
        }
    }

    pub fn with_phase(&self, phase: &str) -> Self {
        let mut next = self.clone();
        next.current_phase = phase.to_string();
        next.updated_at = now_iso();
        next
    }

    pub fn with_change(&self, change_id: Option<String>) -> Self {
        let mut next = self.clone();
        next.current_change_id = change_id;
        next.updated_at = now_iso();
        next
    }

    pub fn with_run(&self, run_id: Option<String>) -> Self {
        let mut next = self.clone();
        next.current_run_id = run_id;
        next.updated_at = now_iso();
        next
    }
}

pub struct StateStore {
    root: PathBuf,
}

impl StateStore {
    pub fn new(cwd: String) -> Self {
        Self {
            root: PathBuf::from(cwd),
        }
    }

    pub fn sdd_dir(&self) -> PathBuf {
        self.root.join(SDD_DIR)
    }

    pub fn state_path(&self) -> PathBuf {
        self.sdd_dir().join(STATE_FILE)
    }

    /// runtime 文件不存在时返回初始状态（未初始化）。
    pub fn read(&self) -> Result<WorkflowState, SddError> {
        Ok(RuntimeStore::new(self.root.to_string_lossy().to_string())
            .read()?
            .state)
    }

    /// 更新 runtime 的 state 节点；不生成独立状态备份文件。
    pub fn write(&self, state: &WorkflowState) -> Result<(), SddError> {
        validate_state(state)?;
        let store = RuntimeStore::new(self.root.to_string_lossy().to_string());
        store.update(|document| document.state = state.clone())?;
        Ok(())
    }

    pub fn update<F>(&self, f: F) -> Result<WorkflowState, SddError>
    where
        F: FnOnce(&mut WorkflowState),
    {
        let mut state = self.read()?;
        f(&mut state);
        state.updated_at = now_iso();
        state.version = state.version.saturating_add(1);
        self.write(&state)?;
        Ok(state)
    }
}

fn validate_state(state: &WorkflowState) -> Result<(), SddError> {
    if let Some(change_id) = state.current_change_id.as_deref() {
        crate::git::isolation::validate_change_id(change_id)?;
    }
    if let Some(run_id) = state.current_run_id.as_deref() {
        validate_run_id(run_id)?;
    }
    let value = serde_json::to_value(state)
        .map_err(|error| SddError::new("E_STATE_CORRUPTED", &format!("序列化状态失败：{error}")))?;
    crate::schema::validate_json("state", &value)
}

pub fn validate_run_id(run_id: &str) -> Result<(), SddError> {
    if run_id.is_empty()
        || !run_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(SddError::new(
            "E_SECURITY_BLOCKED",
            &format!("runId 包含非法字符：{run_id}"),
        ));
    }
    Ok(())
}

pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_iso_epoch(secs)
}

pub fn format_iso_epoch(secs: u64) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

pub fn is_sdd_path(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new(SDD_DIR))
}
